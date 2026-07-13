import org.gradle.api.tasks.Exec

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.grarak.dsvita"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.grarak.dsvita"
        minSdk = 30
        targetSdk = 34
        versionCode = 1
        versionName = "0.9.3"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        getByName("debug") {
            isJniDebuggable = true
        }
        getByName("release") {
            isMinifyEnabled = false
            // Sign release with the debug key so sideload/testing builds install without a
            // separate keystore; swap in a real signingConfig for distribution.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    // The native lib is cross-built by cargo (buildRustDebug/Release below) and dropped
    // into jniLibs; gradle just packages it.
    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    // The .so has no debug-symbol stripping tool on the cross path; keep it as built.
    packaging {
        jniLibs.keepDebugSymbols += "**/libdsvita.so"
    }
}

dependencies {
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.constraintlayout:constraintlayout:2.1.4")
    implementation("androidx.recyclerview:recyclerview:1.3.2")
}

// cargo-driven native build wired into the gradle graph, so `./gradlew assembleDebug`
// builds libdsvita.so too. tools/android_build.sh does the aarch64-host cross setup.
fun rustTask(name: String, profile: String, outDir: String) =
    tasks.register<Exec>(name) {
        workingDir = rootDir.parentFile
        commandLine("tools/android_build.sh", profile)
        val dst = layout.buildDirectory.dir("rustJniLibs/arm64-v8a").get().asFile
        doLast {
            dst.mkdirs()
            copy {
                from(rootDir.parentFile.resolve("target/aarch64-linux-android/$outDir/libdsvita.so"))
                into(dst)
            }
        }
    }

val buildRustDebug = rustTask("buildRustDebug", "release-debug", "release-debug")
val buildRustRelease = rustTask("buildRustRelease", "release", "release")

tasks.whenTaskAdded {
    when (name) {
        "mergeDebugJniLibFolders" -> dependsOn(buildRustDebug)
        "mergeReleaseJniLibFolders" -> dependsOn(buildRustRelease)
    }
}
