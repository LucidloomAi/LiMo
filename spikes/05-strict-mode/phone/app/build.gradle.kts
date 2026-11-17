import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "ai.lucidloom.limo.spike05"
    compileSdk = 36

    defaultConfig {
        applicationId = "ai.lucidloom.limo.spike05"
        // Android 9: the Device Owner calls used here (getSecondaryUsers,
        // DISALLOW_CONFIG_DATE_TIME) all exist from API 28.
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1-spike"
    }

    buildTypes {
        getByName("debug") {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    // Test only. Nothing here ships in the APK.
    testImplementation("junit:junit:4.13.2")
}
