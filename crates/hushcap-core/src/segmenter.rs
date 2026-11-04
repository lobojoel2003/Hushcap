//! Utterance segmentation for live captioning.
//!
//! Whisper-style recognisers work on whole chunks of audio rather than a
//! continuous stream. The [`Segmenter`] turns a continuous 16 kHz stream into
//! utterances:
//!
//! * it opens a segment when the [`Vad`] sees sustained speech, including a
//!   little audio from just before ("pre-roll") so first syllables are not cut;
//! * while someone is talking it periodically hands out a snapshot of the
//!   segment so far ([`SegmentEvent::Partial`]) so the UI can show words as
//!   they are spoken;
//! * it closes the segment after a pause ([`SegmentEvent::Final`]), or splits
//!   very long monologues at the quietest recent moment so captions keep
//!   flowing.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::vad::{Vad, VadConfig, VadFrame};
use crate::{samples_to_ms, SAMPLE_RATE};

/// Tunable parameters of the segmenter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SegmenterConfig {
    /// Voice activity detector settings.
    pub vad: VadConfig,
    /// Continuous speech needed to open a segment, in milliseconds.
    pub min_speech_ms: u32,
    /// Silence that closes a segment, in milliseconds.
    pub end_silence_ms: u32,
    /// Audio kept from before speech was detected, in milliseconds.
    pub pre_roll_ms: u32,
    /// Trailing silence kept at the end of a closed segment, in milliseconds.
    pub keep_trailing_ms: u32,
    /// Longest segment before it is split, in milliseconds.
    pub max_segment_ms: u32,
    /// How often to emit partial snapshots while speech continues
    /// (milliseconds). `0` disables partial results.
    pub partial_interval_ms: u32,
    /// Segments with less speech than this (milliseconds) are discarded as
    /// clicks, coughs or door slams.
    pub min_segment_speech_ms: u32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            vad: VadConfig::default(),
            min_speech_ms: 100,
            end_silence_ms: 650,
            pre_roll_ms: 300,
            keep_trailing_ms: 200,
            max_segment_ms: 12_000,
            partial_interval_ms: 900,
            min_segment_speech_ms: 250,
        }
    }
}

/// Something that happened in the stream.
#[derive(Debug, Clone, PartialEq)]
pub enum SegmentEvent {
    /// Speech started; a new segment is open.
    Started {
        /// Segment identifier, unique per segmenter.
        id: u64,
        /// Start time from the beginning of the stream, in milliseconds.
        start_ms: u64,
    },
    /// Snapshot of an open segment, for interim captions.
    Partial {
        /// Segment identifier.
        id: u64,
        /// Start time in milliseconds.
        start_ms: u64,
        /// All audio of the segment so far.
        audio: Vec<f32>,
    },
    /// A finished utterance.
    Final {
        /// Segment identifier.
        id: u64,
        /// Start time in milliseconds.
        start_ms: u64,
        /// End time in milliseconds.
        end_ms: u64,
        /// How much of the segment the detector considered speech, in milliseconds.
        speech_ms: u64,
        /// The utterance audio.
        audio: Vec<f32>,
    },
    /// An open segment turned out to be too short to be speech.
    Discarded {
        /// Segment identifier.
        id: u64,
    },
}

#[derive(Debug, Clone)]
struct Frame {
    samples: Vec<f32>,
    energy_db: f32,
    is_speech: bool,
}

#[derive(Debug)]
struct Active {
    id: u64,
    start_sample: u64,
    frames: Vec<Frame>,
    silence_run: usize,
    speech_frames: usize,
    frames_since_partial: usize,
}

impl Active {
    fn audio(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(
            self.frames.len() * self.frames.first().map_or(0, |f| f.samples.len()),
        );
        for f in &self.frames {
            out.extend_from_slice(&f.samples);
        }
        out
    }

    fn len_samples(&self) -> usize {
        self.frames.iter().map(|f| f.samples.len()).sum()
    }
}

/// Splits a continuous 16 kHz mono stream into speech segments.
#[derive(Debug)]
pub struct Segmenter {
    config: SegmenterConfig,
    vad: Vad,
    frame_len: usize,
    pending: Vec<f32>,
    /// Samples consumed into complete frames so far.
    processed: u64,
    /// Recent frames while idle (pre-roll + speech run).
    recent: VecDeque<Frame>,
    speech_run: usize,
    active: Option<Active>,
    next_id: u64,
    last_frame: Option<VadFrame>,
}

impl Segmenter {
    /// Create a segmenter.
    pub fn new(config: SegmenterConfig) -> Self {
        let frame_len = config.vad.frame_len();
        Self {
            vad: Vad::new(config.vad.clone()),
            frame_len,
            pending: Vec::with_capacity(frame_len * 4),
            processed: 0,
            recent: VecDeque::new(),
            speech_run: 0,
            active: None,
            next_id: 1,
            last_frame: None,
            config,
        }
    }

    /// The active configuration.
    pub fn config(&self) -> &SegmenterConfig {
        &self.config
    }

    /// Duration of audio processed so far, in milliseconds.
    pub fn position_ms(&self) -> u64 {
        samples_to_ms(self.processed)
    }

    /// Whether someone is talking right now (a segment is open and the
    /// current pause is shorter than 300 ms).
    pub fn is_speaking(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| self.frames_to_ms(a.silence_run) < 300)
    }

    /// Result of the most recent VAD frame, if any.
    pub fn last_frame(&self) -> Option<VadFrame> {
        self.last_frame
    }

    /// Feed 16 kHz mono samples; returns the events they produced.
    pub fn push(&mut self, samples: &[f32]) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        self.pending.extend_from_slice(samples);
        let mut offset = 0;
        while self.pending.len() - offset >= self.frame_len {
            let frame: Vec<f32> = self.pending[offset..offset + self.frame_len].to_vec();
            offset += self.frame_len;
            self.process_frame(frame, &mut events);
        }
        self.pending.drain(..offset);
        events
    }

    /// End of stream: close any open segment.
    pub fn flush(&mut self) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        if let Some(active) = self.active.take() {
            self.finish(active, &mut events);
        }
        self.pending.clear();
        self.recent.clear();
        self.speech_run = 0;
        events
    }

    fn frames_to_ms(&self, frames: usize) -> u32 {
        frames as u32 * self.config.vad.frame_ms
    }

    fn ms_to_frames(&self, ms: u32) -> usize {
        (ms / self.config.vad.frame_ms.max(1)) as usize
    }

    fn process_frame(&mut self, samples: Vec<f32>, events: &mut Vec<SegmentEvent>) {
        let vf = self.vad.process_frame(&samples);
        self.last_frame = Some(vf);
        self.processed += samples.len() as u64;
        let frame = Frame {
            samples,
            energy_db: vf.energy_db,
            is_speech: vf.is_speech,
        };

        match self.active.take() {
            None => self.idle_frame(frame, events),
            Some(active) => self.active_frame(active, frame, events),
        }
    }

    fn idle_frame(&mut self, frame: Frame, events: &mut Vec<SegmentEvent>) {
        self.speech_run = if frame.is_speech {
            self.speech_run + 1
        } else {
            0
        };
        self.recent.push_back(frame);
        let start_frames = self.ms_to_frames(self.config.min_speech_ms).max(1);
        let keep = self.ms_to_frames(self.config.pre_roll_ms) + start_frames;
        while self.recent.len() > keep {
            self.recent.pop_front();
        }
        if self.speech_run < start_frames {
            return;
        }

        let frames: Vec<Frame> = self.recent.drain(..).collect();
        let len: u64 = frames.iter().map(|f| f.samples.len() as u64).sum();
        let speech_frames = frames.iter().filter(|f| f.is_speech).count();
        let id = self.next_id;
        self.next_id += 1;
        let start_sample = self.processed - len;
        events.push(SegmentEvent::Started {
            id,
            start_ms: samples_to_ms(start_sample),
        });
        self.speech_run = 0;
        self.active = Some(Active {
            id,
            start_sample,
            frames,
            silence_run: 0,
            speech_frames,
            frames_since_partial: 0,
        });
    }

    fn active_frame(&mut self, mut active: Active, frame: Frame, events: &mut Vec<SegmentEvent>) {
        if frame.is_speech {
            active.silence_run = 0;
            active.speech_frames += 1;
        } else {
            active.silence_run += 1;
        }
        active.frames.push(frame);
        active.frames_since_partial += 1;

        // A pause long enough to end the utterance.
        if self.frames_to_ms(active.silence_run) >= self.config.end_silence_ms {
            self.finish(active, events);
            return;
        }

        // Too long: split at the quietest recent frame.
        let max_samples = (SAMPLE_RATE as u64 * self.config.max_segment_ms as u64 / 1000) as usize;
        if active.len_samples() >= max_samples {
            active = self.split(active, events);
        }

        // Interim snapshot.
        let interval = self.ms_to_frames(self.config.partial_interval_ms);
        if self.config.partial_interval_ms > 0
            && active.frames_since_partial >= interval.max(1)
            && self.frames_to_ms(active.silence_run) < 300
            && self.frames_to_ms(active.speech_frames) >= 300
        {
            active.frames_since_partial = 0;
            events.push(SegmentEvent::Partial {
                id: active.id,
                start_ms: samples_to_ms(active.start_sample),
                audio: active.audio(),
            });
        }
        self.active = Some(active);
    }

    /// Emit the first part of an over-long segment and continue with the rest.
    fn split(&mut self, mut active: Active, events: &mut Vec<SegmentEvent>) -> Active {
        let n = active.frames.len();
        // Look for a pause in the last 3 seconds, but keep at least 1 s in the first part.
        let window = self.ms_to_frames(3000).min(n.saturating_sub(1));
        let earliest = (n - window).max(self.ms_to_frames(1000)).min(n - 1);
        let split_at = (earliest..n)
            .min_by(|&a, &b| {
                let fa = &active.frames[a];
                let fb = &active.frames[b];
                // Prefer non-speech frames, then lower energy.
                (fa.is_speech as u8, fa.energy_db)
                    .partial_cmp(&(fb.is_speech as u8, fb.energy_db))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(n - 1);

        let rest: Vec<Frame> = active.frames.split_off(split_at + 1);
        let first_len = active.len_samples() as u64;
        let first_speech = active.frames.iter().filter(|f| f.is_speech).count();
        let rest_speech = rest.iter().filter(|f| f.is_speech).count();
        let trailing_silence = rest.iter().rev().take_while(|f| !f.is_speech).count();
        events.push(SegmentEvent::Final {
            id: active.id,
            start_ms: samples_to_ms(active.start_sample),
            end_ms: samples_to_ms(active.start_sample + first_len),
            speech_ms: self.frames_to_ms(first_speech) as u64,
            audio: active.audio(),
        });
        debug_assert!(first_speech > 0 || rest_speech > 0);

        let id = self.next_id;
        self.next_id += 1;
        let start_sample = active.start_sample + first_len;
        events.push(SegmentEvent::Started {
            id,
            start_ms: samples_to_ms(start_sample),
        });
        Active {
            id,
            start_sample,
            frames: rest,
            silence_run: trailing_silence,
            speech_frames: rest_speech,
            frames_since_partial: 0,
        }
    }

    fn finish(&mut self, mut active: Active, events: &mut Vec<SegmentEvent>) {
        // Trim trailing silence, keeping a short tail for natural word endings.
        let keep_trailing = self.ms_to_frames(self.config.keep_trailing_ms);
        let trailing = active
            .frames
            .iter()
            .rev()
            .take_while(|f| !f.is_speech)
            .count();
        let drop = trailing.saturating_sub(keep_trailing);
        let dropped: Vec<Frame> = active.frames.split_off(active.frames.len() - drop);

        // The dropped silence becomes pre-roll for whatever comes next.
        self.recent.clear();
        let pre_roll = self.ms_to_frames(self.config.pre_roll_ms);
        for f in dropped
            .into_iter()
            .rev()
            .take(pre_roll)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            self.recent.push_back(f);
        }
        self.speech_run = 0;

        if self.frames_to_ms(active.speech_frames) < self.config.min_segment_speech_ms {
            events.push(SegmentEvent::Discarded { id: active.id });
            return;
        }
        let len = active.len_samples() as u64;
        events.push(SegmentEvent::Final {
            id: active.id,
            start_ms: samples_to_ms(active.start_sample),
            end_ms: samples_to_ms(active.start_sample + len),
            speech_ms: self.frames_to_ms(active.speech_frames) as u64,
            audio: active.audio(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{noise, silence, speech_like};

    fn finals(events: &[SegmentEvent]) -> Vec<(u64, u64, u64, usize)> {
        events
            .iter()
            .filter_map(|e| match e {
                SegmentEvent::Final {
                    id,
                    start_ms,
                    end_ms,
                    speech_ms,
                    audio,
                } => {
                    assert!(*speech_ms <= end_ms - start_ms);
                    Some((*id, *start_ms, *end_ms, audio.len()))
                }
                _ => None,
            })
            .collect()
    }

    fn quiet_room(ms: u32, seed: u64) -> Vec<f32> {
        noise(0.001, ms, seed)
    }

    fn run(seg: &mut Segmenter, audio: &[f32]) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        for chunk in audio.chunks(480) {
            events.extend(seg.push(chunk));
        }
        events
    }

    #[test]
    fn single_utterance_produces_one_final() {
        let mut seg = Segmenter::new(SegmenterConfig::default());
        let mut audio = quiet_room(1500, 1);
        audio.extend(speech_like(2000, 150.0));
        audio.extend(quiet_room(1500, 2));
        let events = run(&mut seg, &audio);
        let f = finals(&events);
        assert_eq!(
            f.len(),
            1,
            "events: {:?}",
            events
                .iter()
                .map(std::mem::discriminant)
                .collect::<Vec<_>>()
        );
        let (_, start, end, _) = f[0];
        // Speech starts at 1.5 s; pre-roll may pull the start a little earlier.
        assert!((1100..=1600).contains(&start), "start {start}");
        assert!((3400..=3900).contains(&end), "end {end}");
        assert!(matches!(events[0], SegmentEvent::Started { .. }));
    }

    #[test]
    fn partials_are_emitted_while_talking() {
        let mut seg = Segmenter::new(SegmenterConfig::default());
        let mut audio = quiet_room(1000, 3);
        audio.extend(speech_like(4000, 120.0));
        audio.extend(quiet_room(1000, 4));
        let events = run(&mut seg, &audio);
        let partials: Vec<usize> = events
            .iter()
            .filter_map(|e| match e {
                SegmentEvent::Partial { audio, .. } => Some(audio.len()),
                _ => None,
            })
            .collect();
        assert!(partials.len() >= 3, "only {} partials", partials.len());
        assert!(
            partials.windows(2).all(|w| w[1] > w[0]),
            "partials must grow"
        );
    }

    #[test]
    fn two_utterances_separated_by_pause() {
        let mut seg = Segmenter::new(SegmenterConfig::default());
        let mut audio = quiet_room(1000, 5);
        audio.extend(speech_like(1500, 130.0));
        audio.extend(quiet_room(1200, 6));
        audio.extend(speech_like(1500, 210.0));
        audio.extend(quiet_room(1000, 7));
        let f = finals(&run(&mut seg, &audio));
        assert_eq!(f.len(), 2);
        assert!(f[0].0 < f[1].0);
        assert!(f[0].2 <= f[1].1, "segments must not overlap");
    }

    #[test]
    fn long_monologue_is_split() {
        let config = SegmenterConfig {
            max_segment_ms: 5000,
            ..Default::default()
        };
        let mut seg = Segmenter::new(config);
        let mut audio = quiet_room(1000, 8);
        audio.extend(speech_like(12_000, 140.0));
        audio.extend(quiet_room(1500, 9));
        let f = finals(&run(&mut seg, &audio));
        assert!(f.len() >= 3, "expected splits, got {}", f.len());
        for (_, start, end, len) in &f {
            assert!(end - start <= 5100, "segment too long: {}", end - start);
            assert_eq!(
                *len as u64,
                (end - start) * 16,
                "audio length matches timestamps"
            );
        }
        for w in f.windows(2) {
            assert_eq!(w[0].2, w[1].1, "split segments are contiguous");
        }
    }

    #[test]
    fn short_click_is_discarded() {
        let mut seg = Segmenter::new(SegmenterConfig::default());
        let mut audio = quiet_room(1500, 10);
        audio.extend(speech_like(160, 150.0));
        audio.extend(quiet_room(1500, 11));
        let events = run(&mut seg, &audio);
        assert!(finals(&events).is_empty());
        assert!(events
            .iter()
            .any(|e| matches!(e, SegmentEvent::Discarded { .. })));
    }

    #[test]
    fn flush_closes_open_segment() {
        let mut seg = Segmenter::new(SegmenterConfig::default());
        let mut audio = silence(500);
        audio.extend(speech_like(1500, 150.0));
        run(&mut seg, &audio);
        assert!(seg.is_speaking());
        let f = finals(&seg.flush());
        assert_eq!(f.len(), 1);
        assert!(!seg.is_speaking());
    }

    #[test]
    fn position_tracks_input() {
        let mut seg = Segmenter::new(SegmenterConfig::default());
        run(&mut seg, &silence(1234));
        // Whole 20 ms frames only.
        assert_eq!(seg.position_ms(), 1220);
    }
}
