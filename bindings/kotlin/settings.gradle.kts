pluginManagement {
    repositories {
        gradlePluginPortal()
        mavenCentral()
    }
}

plugins {
    // Sonatype's Central Portal accepts Gradle publications through its
    // publisher API (a bundle upload), not a plain PUT to the maven-releases
    // repository -- that returns 404. nmcp is the plugin Sonatype recommends
    // for this, and it is what the Java build gets via
    // central-publishing-maven-plugin.
    id("com.gradleup.nmcp.settings") version "1.6.2"
}

rootProject.name = "ilink-sdk-kotlin"

nmcpSettings {
    centralPortal {
        // The release workflow sets ORG_GRADLE_PROJECT_centralUsername and
        // ...centralPassword, which Gradle maps onto project properties. They
        // are empty for a local build, where publishing is not attempted.
        username = providers.gradleProperty("centralUsername").orNull ?: ""
        password = providers.gradleProperty("centralPassword").orNull ?: ""
        // Release automatically once the deployment validates.
        publishingType = "AUTOMATIC"
    }
}
