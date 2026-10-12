// Iki for Android — Kotlin views over the Rust kernel (src/, loaded as libiki.so).
//
// One Gradle project, no `app/` module and no src/main/java/org/iki/… ladder:
//   kotlin/        app sources (flat; Kotlin doesn't tie directories to packages)
//   kotlin-test/   Robolectric tests, run against the host build of libiki
//   res/           just the launcher icon
// Everything generated — Gradle's build dir, cargo-ndk's jniLibs — lives in
// the workspace target/ dir. scripts/build-android.sh drives the whole build.
plugins {
    id("com.android.application") version "9.4.1"
}

val target: File = rootDir.resolve("../../target")
layout.buildDirectory.set(target.resolve("android/gradle"))

android {
    namespace = "org.iki.shakuhachi"
    compileSdk = 36
    ndkVersion = "30.0.16248370" // strips the Rust .so files when packaging

    defaultConfig {
        applicationId = "org.iki.shakuhachi"
        minSdk = 26 // AAudio
        targetSdk = 36
        versionCode = 3
        versionName = "0.2.0"
    }

    sourceSets {
        getByName("main") {
            manifest.srcFile("AndroidManifest.xml")
            kotlin.setSrcDirs(listOf("kotlin"))
            java.setSrcDirs(emptyList<String>())
            res.setSrcDirs(listOf("res"))
            jniLibs.setSrcDirs(listOf(target.resolve("android/jniLibs")))
        }
        getByName("test") {
            kotlin.setSrcDirs(listOf("kotlin-test"))
            java.setSrcDirs(emptyList<String>())
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true // R8: drops the unused Kotlin stdlib
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
            // Sideload build: signed with the local debug key.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    lint {
        lintConfig = file("lint.xml")
    }

    testOptions {
        unitTests {
            isIncludeAndroidResources = true
            all {
                it.systemProperty("java.library.path", target.resolve("release").path)
                it.systemProperty("iki.screenshots", target.resolve("../dist/android-screenshots").path)
                it.maxHeapSize = "2g"
                // Robolectric (Android 16 sandbox, Java 21) reaches into JDK internals.
                it.jvmArgs(
                    "--add-exports=java.base/jdk.internal.access=ALL-UNNAMED",
                    "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED",
                    "--add-opens=java.base/java.io=ALL-UNNAMED",
                )
                it.testLogging {
                    events("passed", "failed", "skipped")
                    showStandardStreams = true
                    exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
                }
            }
        }
    }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.17")
}
