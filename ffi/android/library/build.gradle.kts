plugins {
    alias(libs.plugins.android.library)
    id("com.vanniktech.maven.publish") version "0.30.0"
}

android {
    namespace = "id.my.rgmtrv.escposdsl"
    compileSdk {
        version = release(37)
    }

    defaultConfig {
        minSdk = 24

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        ndk {
            abiFilters += listOf("armeabi-v7a", "arm64-v8a", "x86", "x86_64")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

val resolvedVersion = project.findProperty("version")?.toString()?.takeIf { it.isNotEmpty() && it != "unspecified" } ?: "0.1.0"

mavenPublishing {
    publishToMavenCentral(
        com.vanniktech.maven.publish.SonatypeHost.CENTRAL_PORTAL,
        automaticRelease = true
    )

    if (System.getenv("JITPACK") != "true") {
        signAllPublications()
    }

    coordinates("id.my.rgmtrv", "escpos-dsl", resolvedVersion)

    pom {
        name.set("escpos-dsl")
        description.set("ESC/POS printer DSL for Android and Rust")
        url.set("https://github.com/RoganMatrivski/escpos-dsl")

        licenses {
            license {
                name.set("MIT License")
                url.set("https://opensource.org/licenses/MIT")
            }
        }

        developers {
            developer {
                id.set("RoganMatrivski")
                name.set("Rogan Matrivski")
            }
        }

        scm {
            connection.set("scm:git:github.com/RoganMatrivski/escpos-dsl.git")
            developerConnection.set("scm:git:ssh://github.com/RoganMatrivski/escpos-dsl.git")
            url.set("https://github.com/RoganMatrivski/escpos-dsl")
        }
    }
}

dependencies {
    implementation(libs.androidx.appcompat)
    implementation(libs.androidx.core.ktx)
    implementation(libs.material)
    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(libs.androidx.junit)
}
