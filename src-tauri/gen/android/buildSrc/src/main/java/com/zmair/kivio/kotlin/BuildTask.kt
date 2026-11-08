import java.io.File
import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

open class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        val executable = """npm""";
        try {
            runTauriCli(executable)
        } catch (e: Exception) {
            if (Os.isFamily(Os.FAMILY_WINDOWS)) {
                // Try different Windows-specific extensions
                val fallbacks = listOf(
                    "$executable.exe",
                    "$executable.cmd",
                    "$executable.bat",
                )
                
                var lastException: Exception = e
                for (fallback in fallbacks) {
                    try {
                        runTauriCli(fallback)
                        return
                    } catch (fallbackException: Exception) {
                        lastException = fallbackException
                    }
                }
                throw lastException
            } else {
                throw e;
            }
        }
    }

    fun runTauriCli(executable: String) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
        // Windows without Developer Mode cannot create the symlink Tauri uses for jniLibs.
        // Cargo already produced libkivio.so. Hard-link it (same volume) and fall back to a copy.
        logger.lifecycle("Hard-linking libkivio.so for $target instead of $executable")
        linkNativeLibrary(rootDirRel, target, release)
    }

    private fun linkNativeLibrary(rootDirRel: String, target: String, release: Boolean) {
        val triple = when (target) {
            "aarch64" -> "aarch64-linux-android"
            "armv7" -> "armv7-linux-androideabi"
            "i686" -> "i686-linux-android"
            "x86_64" -> "x86_64-linux-android"
            else -> throw GradleException("unknown Android target $target")
        }
        val abi = when (target) {
            "aarch64" -> "arm64-v8a"
            "armv7" -> "armeabi-v7a"
            "i686" -> "x86"
            "x86_64" -> "x86_64"
            else -> throw GradleException("unknown Android target $target")
        }
        val profile = if (release) "release" else "debug"
        val tauriDir = File(project.projectDir, rootDirRel)
        val source = File(tauriDir, "target/$triple/$profile/libkivio.so")
        if (!source.isFile) {
            throw GradleException("Missing ${source.absolutePath}. Build the Android library with cargo before Gradle.")
        }
        val destDir = File(project.projectDir, "src/main/jniLibs/$abi")
        if (!destDir.isDirectory && !destDir.mkdirs()) {
            throw GradleException("Could not create ${destDir.absolutePath}")
        }
        val dest = File(destDir, "libkivio.so")
        if (dest.exists() && !dest.delete()) {
            throw GradleException("Could not replace ${dest.absolutePath}")
        }
        try {
            java.nio.file.Files.createLink(dest.toPath(), source.toPath())
        } catch (linkError: Exception) {
            source.copyTo(dest, overwrite = true)
        }
    }
}