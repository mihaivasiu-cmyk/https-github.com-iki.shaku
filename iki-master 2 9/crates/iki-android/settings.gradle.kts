// The Android app is a single Gradle project rooted in this crate, next to the
// Rust JNI bridge it loads. See build.gradle.kts for the flat source layout.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "iki"

// Kotlin's per-project state (.kotlin/) goes to the workspace target/ dir, not into the crate.
gradle.beforeProject {
    extensions.extraProperties["kotlin.project.persistent.dir"] =
        rootDir.resolve("../../target/android/kotlin").canonicalPath
}
