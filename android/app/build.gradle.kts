plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// One version for every platform: the repository's VERSION file.
val appVersion: String = rootProject.file("../VERSION").readText().trim()
val versionParts = appVersion.split("-")[0].split(".").map { it.toInt() }
require(versionParts.size == 3) { "VERSION must be MAJOR.MINOR.PATCH, got $appVersion" }
// 1.2.3 -> 10203. Monotonic for every release as long as minor/patch stay below 100.
val appVersionCode = versionParts[0] * 10_000 + versionParts[1] * 100 + versionParts[2]
val buildNumber: String = System.getenv("HUSHCAP_BUILD_NUMBER")?.takeIf { it.isNotBlank() } ?: "0"
val gitSha: String = System.getenv("HUSHCAP_GIT_SHA")?.takeIf { it.isNotBlank() }?.take(10)
    ?: providers.exec {
        commandLine("git", "rev-parse", "--short=10", "HEAD")
        isIgnoreExitValue = true
    }.standardOutput.asText.get().trim().ifEmpty { "unknown" }

val keystorePath: String? = System.getenv("ANDROID_KEYSTORE_PATH")?.takeIf { it.isNotBlank() }

android {
    namespace = "io.github.lobojoel2003.hushcap"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.lobojoel2003.hushcap"
        minSdk = 26
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersion
        buildConfigField("String", "BUILD_NUMBER", "\"$buildNumber\"")
        buildConfigField("String", "GIT_SHA", "\"$gitSha\"")
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            // JNA (used by Vosk) relies on reflection; keep the code unshrunk.
            isMinifyEnabled = false
            isShrinkResources = false
            if (keystorePath != null) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }

    lint {
        abortOnError = true
        warningsAsErrors = false
        checkReleaseBuilds = true
        disable += setOf("GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion", "OldTargetApi")
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

kotlin {
    jvmToolchain(17)
}

base {
    archivesName.set("hushcap-$appVersion")
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.service)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material.icons.core)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.vosk.android)
    // Vosk loads its native library through JNA, which must be the Android (AAR) build.
    implementation("net.java.dev.jna:jna:${libs.versions.jna.get()}@aar")
    debugImplementation(libs.androidx.compose.ui.tooling)

    testImplementation(libs.junit)
    testImplementation(libs.json)
}
