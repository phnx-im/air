// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation

/// The App Group container shared by the app, the notification service and the
/// share extension.
///
/// All three open the same database and cache directories, so the paths and the
/// file protection applied to them have to agree.
enum AppGroup {
    /// The staging build of each target is only entitled to the staging App
    /// Group, where requesting the production group returns nil (and the other
    /// way around). The bundle identifier carries the flavor for all three
    /// targets: `ms.air[.staging][.nse|.share]`.
    static let identifier: String =
        Bundle.main.bundleIdentifier?.contains(".staging") == true
        ? "group.ms.air.staging" : "group.ms.air"

    static func containerURL() -> URL? {
        FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: identifier)
    }

    /// The databases directory, which is not backed up to iCloud and stays
    /// readable while the device is locked.
    ///
    /// Only the app creates it.
    static func databasesDirectory(create: Bool) -> URL? {
        guard let containerURL = containerURL() else {
            return nil
        }

        // Library/Application Support for persistent, non-user-visible data
        let dbsURL =
            containerURL
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent("Databases", isDirectory: true)

        if create {
            guard createBackupExcludedDirectory(at: dbsURL) else {
                return nil
            }
        } else if !FileManager.default.fileExists(atPath: dbsURL.path) {
            return nil
        }

        applyProtection(dbsURL)
        // Protection is also applied to the temp directory, because sqlite
        // uses it to write statement journal files:
        // <https://sqlite.org/tempfiles.html>
        applyProtection(URL(fileURLWithPath: NSTemporaryDirectory()))
        return dbsURL
    }

    /// The cache directory shared between the app and the extensions. Created
    /// by whoever gets there first, so the backup exclusion is applied on
    /// every path that creates it.
    static func sharedCachesDirectory(create: Bool) -> URL? {
        guard let containerURL = containerURL() else {
            return nil
        }
        let cachesURL = containerURL.appendingPathComponent(
            "Caches", isDirectory: true)
        if create, !createBackupExcludedDirectory(at: cachesURL) {
            return nil
        }
        return cachesURL
    }

    /// Allows writing to the given URL while the device is locked
    static func applyProtection(_ url: URL) {
        try? FileManager.default.setAttributes(
            [
                .protectionKey: FileProtectionType
                    .completeUntilFirstUserAuthentication
            ],
            ofItemAtPath: url.path
        )
    }

    private static func createBackupExcludedDirectory(at url: URL) -> Bool {
        do {
            try FileManager.default.createDirectory(
                at: url, withIntermediateDirectories: true)
        } catch {
            return false
        }
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var url = url
        try? url.setResourceValues(values)
        return true
    }
}

/// Names shared between the targets, and for the method channel with the Dart
/// side (see `app/lib/platform/method_channel.dart`).
enum AppChannel {
    static let name = "ms.air/channel"

    /// Darwin notification posted after store notifications were written to
    /// the database, so a running app reloads its stores.
    static let storeNotificationsPendingName =
        "ms.air.store-notifications-pending"
}
