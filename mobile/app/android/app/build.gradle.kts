import java.util.Properties

plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// The release key lives outside the repository (android/SIGNING.md). A release
// built without it would be debug-signed, and a debug-signed build cannot
// update a release-signed install — Android makes you uninstall, which throws
// away the pairing token. So a release without the key fails here instead.
val keyProperties = Properties().apply {
    val f = rootProject.file("key.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}

android {
    namespace = "ca.brownfamilysports.terminal_delight"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        // The gateway's `pair` verb addresses the app by this id.
        applicationId = "ca.brownfamilysports.terminaldelight"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (keyProperties.isNotEmpty()) {
            create("release") {
                keyAlias = keyProperties["keyAlias"] as String
                keyPassword = keyProperties["keyPassword"] as String
                storeFile = file(keyProperties["storeFile"] as String)
                storePassword = keyProperties["storePassword"] as String
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
        }
    }
}

gradle.taskGraph.whenReady {
    val releasing = allTasks.any { it.name.contains("Release") }
    if (releasing && keyProperties.isEmpty()) {
        throw GradleException(
            "No android/key.properties: a release would be debug-signed and could not " +
                "update the installed app. See android/SIGNING.md."
        )
    }
}

flutter {
    source = "../.."
}
