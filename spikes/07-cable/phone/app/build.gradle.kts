import java.security.SecureRandom
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// The release build is signed with a throwaway key, made here the first time
// and kept in phone/throwaway-key/ (git ignored, never committed): never
// LiMo's own key and never Android's debug key. It signs nothing but this
// test app. A random password is made with it and kept beside it, so no
// password is written in this file. Deleting the folder makes a new key at
// the next build; an app signed with the old one must then be removed from
// the phone before the new one installs.
val throwawayDir = rootProject.file("throwaway-key")
val throwawayStore = File(throwawayDir, "spike07-throwaway.p12")
val throwawayPassword = File(throwawayDir, "password.txt")
if (!throwawayStore.isFile || !throwawayPassword.isFile) {
    throwawayDir.mkdirs()
    throwawayStore.delete()
    val bytes = ByteArray(24).also { SecureRandom().nextBytes(it) }
    throwawayPassword.writeText(bytes.joinToString("") { "%02x".format(it) })
    val windows = System.getProperty("os.name").lowercase().contains("windows")
    val keytool = File(System.getProperty("java.home"), "bin/keytool" + if (windows) ".exe" else "")
    val made = ProcessBuilder(
        keytool.path, "-genkeypair", "-storetype", "PKCS12",
        "-keystore", throwawayStore.path, "-alias", "spike07",
        "-keyalg", "RSA", "-keysize", "2048", "-validity", "365",
        "-dname", "CN=LiMo spike 07 throwaway key, O=Test only",
        "-storepass:file", throwawayPassword.path, "-keypass:file", throwawayPassword.path,
        "-noprompt",
    ).inheritIO().start().waitFor()
    check(made == 0) { "keytool could not make the throwaway key" }
}

android {
    namespace = "ai.lucidloom.limo.spike07"
    compileSdk = 36

    defaultConfig {
        applicationId = "ai.lucidloom.limo.spike07"
        // Android 8.0, as LiMo's child form.
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1-spike07"
    }

    signingConfigs {
        create("throwaway") {
            storeFile = throwawayStore
            storeType = "PKCS12"
            storePassword = throwawayPassword.readText().trim()
            keyAlias = "spike07"
            keyPassword = throwawayPassword.readText().trim()
        }
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("throwaway")
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
