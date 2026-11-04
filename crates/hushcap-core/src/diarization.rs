//! Online speaker clustering ("who spoke when").
//!
//! Each finished utterance is turned into a speaker embedding by a neural
//! model (see `hushcap-engine`). This module groups those embeddings into
//! speakers as the conversation happens:
//!
//! * an utterance joins the most similar known speaker if the cosine
//!   similarity is above a threshold, otherwise it starts a new speaker;
//! * speaker profiles are running averages, so they improve as people talk;
//! * if two profiles drift together (the same person was split in two early
//!   on), they are merged and the caller is told to relabel past captions;
//! * very short utterances never create new speakers, because their
//!   embeddings are unreliable.
//!
//! Unlike fixed-slot approaches there is no small cap on the number of
//! speakers: a family dinner with eight people gets eight labels.

use serde::{Deserialize, Serialize};

/// Tunable parameters of the clusterer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClusterConfig {
    /// Minimum cosine similarity for an utterance to join an existing speaker.
    pub threshold: f32,
    /// Two speaker profiles at least this similar are merged.
    pub merge_threshold: f32,
    /// Utterances shorter than this (milliseconds) can join a speaker but
    /// never create one.
    pub min_new_speaker_ms: u64,
    /// Upper bound on distinct speakers; beyond it utterances join the nearest.
    pub max_speakers: usize,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            merge_threshold: 0.72,
            min_new_speaker_ms: 1200,
            max_speakers: 64,
        }
    }
}

impl ClusterConfig {
    /// Map a user-facing "speaker separation" setting (0 = merge freely,
    /// 1 = split eagerly) to a similarity threshold.
    pub fn with_separation(mut self, separation: f32) -> Self {
        let s = separation.clamp(0.0, 1.0);
        self.threshold = 0.32 + 0.36 * s; // 0.32 … 0.68
        self.merge_threshold = (self.threshold + 0.22).min(0.9);
        self
    }
}

/// Outcome of assigning one utterance.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// The speaker the utterance belongs to, if it could be determined.
    pub speaker: Option<u32>,
    /// Whether this utterance created a new speaker.
    pub is_new: bool,
    /// Similarity to the chosen speaker (or best candidate).
    pub similarity: f32,
    /// Speakers merged as a consequence: `(from, into)`; captions labelled
    /// `from` should be relabelled `into`.
    pub merges: Vec<(u32, u32)>,
}

#[derive(Debug, Clone)]
struct Profile {
    id: u32,
    sum: Vec<f32>,
    centroid: Vec<f32>,
    count: u32,
}

impl Profile {
    fn add(&mut self, embedding: &[f32], weight: f32) {
        for (s, &e) in self.sum.iter_mut().zip(embedding) {
            *s += e * weight;
        }
        self.count += 1;
        self.centroid = normalized(&self.sum);
    }
}

/// Incremental speaker clustering over L2-normalised embeddings.
#[derive(Debug, Clone)]
pub struct SpeakerClusterer {
    config: ClusterConfig,
    profiles: Vec<Profile>,
    next_id: u32,
    dim: Option<usize>,
}

impl SpeakerClusterer {
    /// Create an empty clusterer.
    pub fn new(config: ClusterConfig) -> Self {
        Self {
            config,
            profiles: Vec::new(),
            next_id: 1,
            dim: None,
        }
    }

    /// The active configuration.
    pub fn config(&self) -> &ClusterConfig {
        &self.config
    }

    /// Change thresholds without forgetting known speakers.
    pub fn set_config(&mut self, config: ClusterConfig) {
        self.config = config;
    }

    /// Number of known speakers.
    pub fn speaker_count(&self) -> usize {
        self.profiles.len()
    }

    /// Identifiers of known speakers.
    pub fn speakers(&self) -> Vec<u32> {
        self.profiles.iter().map(|p| p.id).collect()
    }

    /// Forget all speakers.
    pub fn reset(&mut self) {
        self.profiles.clear();
        self.next_id = 1;
        self.dim = None;
    }

    /// Assign an utterance's embedding to a speaker.
    ///
    /// `duration_ms` is the utterance length; longer utterances give more
    /// reliable embeddings and weigh more in the speaker profile.
    pub fn assign(&mut self, embedding: &[f32], duration_ms: u64) -> Assignment {
        let e = normalized(embedding);
        if e.iter().all(|v| *v == 0.0) || self.dim.is_some_and(|d| d != e.len()) {
            return Assignment {
                speaker: None,
                is_new: false,
                similarity: 0.0,
                merges: Vec::new(),
            };
        }
        self.dim = Some(e.len());
        let weight = (duration_ms as f32 / 1000.0).clamp(0.3, 6.0);
        let best = self
            .profiles
            .iter()
            .enumerate()
            .map(|(i, p)| (i, cosine(&p.centroid, &e)))
            .max_by(|a, b| a.1.total_cmp(&b.1));

        let short = duration_ms < self.config.min_new_speaker_ms;
        let full = self.profiles.len() >= self.config.max_speakers;

        match best {
            Some((idx, sim)) if sim >= self.config.threshold || (full && !short) => {
                let id = self.profiles[idx].id;
                // Short utterances inform the label but do not move the profile.
                if !short {
                    self.profiles[idx].add(&e, weight);
                }
                let merges = if short {
                    Vec::new()
                } else {
                    self.merge_close(idx)
                };
                let speaker = merges
                    .iter()
                    .find(|(from, _)| *from == id)
                    .map(|(_, into)| *into)
                    .unwrap_or(id);
                Assignment {
                    speaker: Some(speaker),
                    is_new: false,
                    similarity: sim,
                    merges,
                }
            }
            Some((idx, sim)) if short => {
                // Too short to start a new speaker. Accept a looser match.
                let speaker = (sim >= self.config.threshold * 0.75).then(|| self.profiles[idx].id);
                Assignment {
                    speaker,
                    is_new: false,
                    similarity: sim,
                    merges: Vec::new(),
                }
            }
            None if short => Assignment {
                speaker: None,
                is_new: false,
                similarity: 0.0,
                merges: Vec::new(),
            },
            other => {
                let id = self.next_id;
                self.next_id += 1;
                let mut profile = Profile {
                    id,
                    sum: vec![0.0; e.len()],
                    centroid: e.clone(),
                    count: 0,
                };
                profile.add(&e, weight);
                self.profiles.push(profile);
                Assignment {
                    speaker: Some(id),
                    is_new: true,
                    similarity: other.map_or(0.0, |(_, s)| s),
                    merges: Vec::new(),
                }
            }
        }
    }

    /// Merge any profile that has become too similar to the one at `idx`.
    fn merge_close(&mut self, idx: usize) -> Vec<(u32, u32)> {
        let mut merges = Vec::new();
        let mut target = idx;
        loop {
            let centroid = self.profiles[target].centroid.clone();
            let candidate = self
                .profiles
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != target)
                .map(|(i, p)| (i, cosine(&p.centroid, &centroid)))
                .filter(|(_, s)| *s >= self.config.merge_threshold)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let Some((other, _)) = candidate else { break };
            // Keep the older (lower) id so labels stay stable.
            let (keep, gone) = if self.profiles[other].id < self.profiles[target].id {
                (other, target)
            } else {
                (target, other)
            };
            let removed = self.profiles[gone].clone();
            {
                let kept = &mut self.profiles[keep];
                for (s, r) in kept.sum.iter_mut().zip(&removed.sum) {
                    *s += r;
                }
                kept.count += removed.count;
                kept.centroid = normalized(&kept.sum);
            }
            merges.push((removed.id, self.profiles[keep].id));
            self.profiles.remove(gone);
            target = if keep > gone { keep - 1 } else { keep };
        }
        merges
    }
}

/// Cosine similarity of two vectors (0 if either is zero).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (&x, &y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// L2-normalise a vector (zero vectors stay zero).
pub fn normalized(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n == 0.0 || !n.is_finite() {
        vec![0.0; v.len()]
    } else {
        v.iter().map(|x| x / n).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic "speaker" vectors: a random direction per speaker plus
    /// small per-utterance noise.
    fn voice(speaker: u64, utterance: u64, dim: usize, noise: f32) -> Vec<f32> {
        let mut s = speaker.wrapping_mul(0x9E3779B97F4A7C15) | 1;
        let mut u = (speaker * 1000 + utterance).wrapping_mul(0xD1B54A32D192ED03) | 1;
        let next = |st: &mut u64| {
            *st ^= *st << 13;
            *st ^= *st >> 7;
            *st ^= *st << 17;
            (*st as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0
        };
        (0..dim)
            .map(|_| next(&mut s) + noise * next(&mut u))
            .collect()
    }

    #[test]
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        let n = normalized(&[3.0, 4.0]);
        assert!((n[0] - 0.6).abs() < 1e-6 && (n[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn separates_many_speakers() {
        // Well beyond the ~4 speakers some captioning apps can handle.
        let mut c = SpeakerClusterer::new(ClusterConfig::default());
        let speakers = 8u64;
        let mut labels = std::collections::HashMap::new();
        for round in 0..6 {
            for s in 0..speakers {
                let a = c.assign(&voice(s, round, 192, 0.35), 3000);
                let label = a.speaker.expect("long utterance gets a label");
                let prev = labels.insert(s, label);
                if let Some(prev) = prev {
                    assert_eq!(prev, label, "speaker {s} changed label in round {round}");
                }
            }
        }
        assert_eq!(c.speaker_count(), speakers as usize);
        let distinct: std::collections::HashSet<_> = labels.values().collect();
        assert_eq!(distinct.len(), speakers as usize);
    }

    #[test]
    fn short_utterance_never_creates_speaker() {
        let mut c = SpeakerClusterer::new(ClusterConfig::default());
        let a = c.assign(&voice(1, 0, 64, 0.1), 400);
        assert_eq!(a.speaker, None);
        assert_eq!(c.speaker_count(), 0);

        let first = c.assign(&voice(1, 1, 64, 0.1), 3000).speaker;
        let short_same = c.assign(&voice(1, 2, 64, 0.1), 400);
        assert_eq!(short_same.speaker, first);
        let short_other = c.assign(&voice(2, 0, 64, 0.1), 400);
        assert_eq!(short_other.speaker, None);
        assert_eq!(c.speaker_count(), 1);
    }

    #[test]
    fn split_speaker_is_merged_back() {
        let config = ClusterConfig {
            threshold: 0.95, // absurdly strict: forces the same voice into two profiles
            merge_threshold: 0.8,
            ..Default::default()
        };
        let mut c = SpeakerClusterer::new(config);
        let a = c.assign(&voice(7, 0, 128, 0.5), 3000);
        let b = c.assign(&voice(7, 1, 128, 0.5), 3000);
        assert!(b.is_new, "strict threshold should split");
        assert_ne!(a.speaker, b.speaker);
        // Relax the threshold: the next utterance joins and triggers a merge.
        c.set_config(ClusterConfig {
            threshold: 0.5,
            merge_threshold: 0.8,
            ..Default::default()
        });
        let mut merges = Vec::new();
        for i in 2..8 {
            let r = c.assign(&voice(7, i, 128, 0.5), 3000);
            merges.extend(r.merges);
            assert_eq!(r.speaker, a.speaker, "labels converge on the older id");
        }
        assert_eq!(merges, vec![(b.speaker.unwrap(), a.speaker.unwrap())]);
        assert_eq!(c.speaker_count(), 1);
    }

    #[test]
    fn max_speakers_is_respected() {
        let config = ClusterConfig {
            max_speakers: 3,
            ..Default::default()
        };
        let mut c = SpeakerClusterer::new(config);
        for s in 0..6 {
            assert!(c.assign(&voice(s, 0, 64, 0.0), 3000).speaker.is_some());
        }
        assert_eq!(c.speaker_count(), 3);
    }

    #[test]
    fn mismatched_dimensions_are_ignored() {
        let mut c = SpeakerClusterer::new(ClusterConfig::default());
        c.assign(&voice(1, 0, 64, 0.0), 3000);
        let a = c.assign(&voice(1, 0, 32, 0.0), 3000);
        assert_eq!(a.speaker, None);
        assert_eq!(c.assign(&[0.0; 64], 3000).speaker, None);
    }

    #[test]
    fn separation_setting_is_monotonic() {
        let low = ClusterConfig::default().with_separation(0.0);
        let high = ClusterConfig::default().with_separation(1.0);
        assert!(low.threshold < high.threshold);
        assert!(high.merge_threshold > high.threshold);
    }
}
