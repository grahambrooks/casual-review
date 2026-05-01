plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "2.2.0"
    id("org.jetbrains.intellij.platform") version "2.6.0"
}

group = "com.casualreview"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdeaCommunity("2024.2")
        // Gson is bundled with the IDE; no extra dep needed for JSON parsing.
    }
}

intellijPlatform {
    pluginConfiguration {
        id = "com.casualreview.jetbrains"
        name = "Casual Review"
        version = project.version.toString()
        description = "Read, write, and sync code-review comments stored in git notes via the cr CLI."
        vendor {
            name = "casual-review"
            url = "https://github.com/grahambrooks/casual-review"
        }
        ideaVersion {
            sinceBuild = "242"
            untilBuild = "243.*"
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21)
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

tasks {
    wrapper {
        gradleVersion = "9.0.0"
    }
}
