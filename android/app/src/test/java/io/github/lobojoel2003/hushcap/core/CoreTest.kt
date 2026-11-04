package io.github.lobojoel2003.hushcap.core

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SpeakerClustererTest {
    /** A random direction per speaker plus per-utterance noise. */
    private fun voice(speaker: Int, utterance: Int, dim: Int = 128, noise: Float = 0.35f): FloatArray {
        val base = java.util.Random(speaker * 7919L)
        val jitter = java.util.Random(speaker * 104729L + utterance)
        return FloatArray(dim) { (base.nextGaussian() + noise * jitter.nextGaussian()).toFloat() }
    }

    @Test
    fun separatesManySpeakers() {
        val c = SpeakerClusterer()
        val labels = mutableMapOf<Int, Int>()
        repeat(5) { round ->
            for (s in 0 until 8) {
                val label = c.assign(voice(s, round), 3000).speaker!!
                labels.put(s, label)?.let { assertEquals("speaker $s round $round", it, label) }
            }
        }
        assertEquals(8, c.speakerCount)
        assertEquals(8, labels.values.toSet().size)
    }

    @Test
    fun shortUtterancesNeverCreateSpeakers() {
        val c = SpeakerClusterer()
        assertNull(c.assign(voice(1, 0), 400).speaker)
        val first = c.assign(voice(1, 1), 3000).speaker
        assertEquals(first, c.assign(voice(1, 2), 400).speaker)
        assertNull(c.assign(voice(2, 0), 400).speaker)
        assertEquals(1, c.speakerCount)
    }

    @Test
    fun splitSpeakerIsMerged() {
        val c = SpeakerClusterer(threshold = 0.95f, mergeThreshold = 0.8f)
        val a = c.assign(voice(7, 0, noise = 0.5f), 3000).speaker
        val b = c.assign(voice(7, 1, noise = 0.5f), 3000)
        assertTrue(b.isNew)
        c.threshold = 0.5f
        val merges = mutableListOf<Pair<Int, Int>>()
        for (i in 2 until 8) {
            val r = c.assign(voice(7, i, noise = 0.5f), 3000)
            merges += r.merges
            assertEquals(a, r.speaker)
        }
        assertEquals(listOf(b.speaker!! to a!!), merges)
        assertEquals(1, c.speakerCount)
    }

    @Test
    fun separationIsMonotonic() {
        val c = SpeakerClusterer()
        c.setSeparation(0f)
        val low = c.threshold
        c.setSeparation(1f)
        assertTrue(c.threshold > low)
        assertTrue(c.mergeThreshold > c.threshold)
    }

    @Test
    fun ignoresBadVectors() {
        val c = SpeakerClusterer()
        c.assign(voice(1, 0, dim = 64), 3000)
        assertNull(c.assign(voice(1, 0, dim = 32), 3000).speaker)
        assertNull(c.assign(FloatArray(64), 3000).speaker)
    }
}

class ExporterTest {
    private val captions = listOf(
        Caption(1, AudioSource.MICROPHONE, 1, "Hello, how are you feeling today?", 1_500, 3_250),
        Caption(2, AudioSource.SYSTEM, 2, "Much better <thanks> & rested.", 3_661_005, 3_663_000),
        Caption(3, AudioSource.SYSTEM, null, "[laughter]", 3_700_000, 3_700_000),
    )
    private val names = mapOf(1 to "Dr. Rivera", 2 to "  ")

    @Test
    fun timestamps() {
        assertEquals("01:02:03,004", timestamp(3_723_004, ','))
        assertEquals("1:05", clock(65_000))
        assertEquals("1:02:05", clock(3_725_000))
    }

    @Test
    fun srtMatchesDesktopFormat() {
        val expected = "1\n00:00:01,500 --> 00:00:03,250\nDr. Rivera: Hello, how are you feeling today?\n\n" +
            "2\n01:01:01,005 --> 01:01:03,000\nSpeaker 2: Much better <thanks> & rested.\n\n" +
            "3\n01:01:40,000 --> 01:01:40,001\n[laughter]\n\n"
        assertEquals(expected, TranscriptExporter.export(captions, names, ExportFormat.SRT))
    }

    @Test
    fun vttEscapesAndUsesVoiceTags() {
        val vtt = TranscriptExporter.export(captions, names, ExportFormat.VTT)
        assertTrue(vtt.startsWith("WEBVTT\n\n00:00:01.500 --> 00:00:03.250\n<v Dr. Rivera>Hello"))
        assertTrue(vtt.contains("<v Speaker 2>Much better &lt;thanks&gt; &amp; rested."))
    }

    @Test
    fun txtAndMarkdown() {
        val txt = TranscriptExporter.export(captions, names, ExportFormat.TXT)
        assertTrue(txt.startsWith("[0:01] Dr. Rivera: Hello"))
        val plain = TranscriptExporter.export(captions, names, ExportFormat.TXT, includeSpeakers = false, includeTimestamps = false)
        assertEquals("Hello, how are you feeling today?\nMuch better <thanks> & rested.\n[laughter]\n", plain)
        val md = TranscriptExporter.export(captions, names, ExportFormat.MARKDOWN, title = "Visit")
        assertTrue(md.startsWith("# Visit\n\n**Dr. Rivera** _0:01_: Hello"))
    }
}

class VoskResultTest {
    @Test
    fun parsesFinalWithSpeakerVector() {
        val r = VoskResult.parseFinal("""{"spk":[0.5,-0.25,1.0],"spk_frames":312,"text":"hello there [unk] friend"}""")
        assertEquals("Hello there friend", r.text)
        assertEquals(312, r.speakerFrames)
        assertEquals(3, r.speakerVector!!.size)
        assertEquals(-0.25f, r.speakerVector!![1])
    }

    @Test
    fun parsesPartialAndGarbage() {
        assertEquals("Testing one two", VoskResult.parsePartial("""{"partial" : "testing one two"}"""))
        assertEquals("", VoskResult.parsePartial("not json"))
        val empty = VoskResult.parseFinal("""{"text" : ""}""")
        assertEquals("", empty.text)
        assertNull(empty.speakerVector)
    }

    @Test
    fun sentenceEnding() {
        assertEquals("Hi there.", TextCleaner.asSentence("Hi there"))
        assertEquals("Really?", TextCleaner.asSentence("Really?"))
        assertEquals("", TextCleaner.asSentence(""))
    }
}

class ZipInstallerTest {
    private fun zip(vararg entries: Pair<String, String>): ByteArray {
        val bytes = ByteArrayOutputStream()
        ZipOutputStream(bytes).use { z ->
            for ((name, content) in entries) {
                z.putNextEntry(ZipEntry(name))
                z.write(content.toByteArray())
                z.closeEntry()
            }
        }
        return bytes.toByteArray()
    }

    @Test
    fun stripsTopLevelFolder() {
        val dir = Files.createTempDirectory("hushcap").toFile()
        val dest = File(dir, "model")
        val archive = zip(
            "vosk-model-small-en-us-0.15/am/final.mdl" to "am",
            "vosk-model-small-en-us-0.15/conf/model.conf" to "conf",
        )
        var progress = 0L
        ZipInstaller.install(ByteArrayInputStream(archive), dest) { progress = it }
        assertTrue(File(dest, "am/final.mdl").isFile)
        assertTrue(ZipInstaller.looksInstalled(dest, ZipInstaller.SPEECH_MODEL_MARKERS))
        assertFalse(ZipInstaller.looksInstalled(dest, ZipInstaller.SPEAKER_MODEL_MARKERS))
        assertNotEquals(0L, progress)
        assertFalse(File(dir, "model.installing").exists())
    }

    @Test(expected = IOException::class)
    fun rejectsZipSlip() {
        val dir = Files.createTempDirectory("hushcap").toFile()
        ZipInstaller.install(ByteArrayInputStream(zip("../evil.txt" to "x")), File(dir, "model"))
    }

    @Test
    fun catalogIsConsistent() {
        assertEquals(1, ModelCatalog.models.count { it.kind == ModelKind.SPEECH && it.recommended })
        assertEquals(1, ModelCatalog.models.count { it.kind == ModelKind.SPEAKER && it.recommended })
        assertEquals(ModelCatalog.models.size, ModelCatalog.models.map { it.id }.toSet().size)
        for (m in ModelCatalog.models) {
            assertTrue(m.url.startsWith("https://alphacephei.com/vosk/models/"))
            assertTrue(m.url.endsWith("${m.id}.zip"))
        }
    }
}
