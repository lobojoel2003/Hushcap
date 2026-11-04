package io.github.lobojoel2003.hushcap.core

import java.io.File
import java.io.IOException
import java.io.InputStream
import java.util.zip.ZipInputStream

/**
 * Unpacks model archives safely.
 *
 * Vosk models ship as zip files containing a single top-level folder. That
 * folder is stripped so the model lands directly in [destination]. Entries
 * that would escape the destination ("zip slip") are rejected.
 */
object ZipInstaller {
    /** Files that identify an unpacked Vosk speech model. */
    val SPEECH_MODEL_MARKERS = listOf("am/final.mdl", "conf/model.conf")

    /** Files that identify an unpacked Vosk speaker model. */
    val SPEAKER_MODEL_MARKERS = listOf("final.ext.raw", "mfcc.conf")

    fun install(
        input: InputStream,
        destination: File,
        onProgress: (bytes: Long) -> Unit = {},
    ) {
        val staging = File(destination.parentFile, destination.name + ".installing")
        staging.deleteRecursively()
        if (!staging.mkdirs()) throw IOException("Could not create ${staging.path}")
        val root = staging.canonicalFile
        var written = 0L
        try {
            ZipInputStream(input.buffered()).use { zip ->
                val entries = mutableListOf<String>()
                var entry = zip.nextEntry
                while (entry != null) {
                    val name = entry.name.replace('\\', '/')
                    entries.add(name)
                    val target = File(root, name).canonicalFile
                    if (!target.path.startsWith(root.path + File.separator) && target != root) {
                        throw IOException("Unsafe path in archive: $name")
                    }
                    if (entry.isDirectory) {
                        target.mkdirs()
                    } else {
                        target.parentFile?.mkdirs()
                        target.outputStream().use { out ->
                            val buf = ByteArray(64 * 1024)
                            while (true) {
                                val n = zip.read(buf)
                                if (n < 0) break
                                out.write(buf, 0, n)
                                written += n
                                onProgress(written)
                            }
                        }
                    }
                    zip.closeEntry()
                    entry = zip.nextEntry
                }
                if (entries.isEmpty()) throw IOException("The archive is empty")
            }
            // Strip a single top-level folder.
            val children = staging.listFiles().orEmpty()
            val contentRoot = if (children.size == 1 && children[0].isDirectory) children[0] else staging
            destination.deleteRecursively()
            if (!contentRoot.renameTo(destination)) {
                contentRoot.copyRecursively(destination, overwrite = true)
            }
        } finally {
            staging.deleteRecursively()
        }
    }

    /** Whether [dir] holds an unpacked model with all of [markers]. */
    fun looksInstalled(dir: File, markers: List<String>): Boolean =
        dir.isDirectory && markers.all { File(dir, it).isFile }
}
