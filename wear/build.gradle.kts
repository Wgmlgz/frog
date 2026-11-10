import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.wgmlgz.frog.wear"
    compileSdk = 37
    defaultConfig {
        applicationId = "com.wgmlgz.frog"
        minSdk = 30
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }
    signingConfigs {
        val keystoreFile = rootProject.file("keystore.properties")
        if (keystoreFile.exists()) {
            create("release") {
                val credentials = Properties().apply { keystoreFile.inputStream().use { load(it) } }
                keyAlias = credentials.getProperty("keyAlias")
                keyPassword = credentials.getProperty("keyPassword", credentials.getProperty("password"))
                storeFile = file(credentials.getProperty("storeFile"))
                storePassword = credentials.getProperty("storePassword", credentials.getProperty("password"))
            }
        }
    }
    buildTypes {
        getByName("release") {
            if (rootProject.file("keystore.properties").exists()) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_11
    }
}

dependencies {
    implementation("com.google.android.gms:play-services-wearable:20.0.1")
    implementation("androidx.wear.tiles:tiles:1.6.2")
    implementation("androidx.wear.protolayout:protolayout:1.4.2")
    implementation("androidx.wear:wear-remote-interactions:1.2.0")
    implementation("com.google.guava:guava:33.4.8-android")
}
