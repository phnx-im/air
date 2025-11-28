//
//  NotificationService.swift
//  NotificationService
//

import Foundation
import UserNotifications
import os

private let kProtectedBlockedCategory = "protected-blocked"
private let log = Logger(
    subsystem: Bundle.main.bundleIdentifier ?? "NotificationService", category: "NSE")

struct IncomingNotificationContent: Codable {
    let title: String
    let body: String
    let data: String
    let path: String
    let logFilePath: String
}

struct NotificationBatch: Codable {
    let badgeCount: UInt32
    let removals: [String]
    let additions: [NotificationContent]
}

struct NotificationContent: Codable {
    let identifier: UUID
    let title: String
    let body: String
    let chatId: ChatId?
}

struct ChatId: Codable {
    let uuid: UUID
}

class NotificationService: UNNotificationServiceExtension {
    
    var contentHandler: ((UNNotificationContent) -> Void)?
    
    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        
        log.info("Received notification")
        self.contentHandler = contentHandler
        
        guard
            let incomingNotification =
                (request.content.mutableCopy() as? UNMutableNotificationContent)
        else {
            self.suppressNotifications()
            return
        }
        
        // Extract the "data" field from the push notification payload
        let userInfo = request.content.userInfo
        guard let data = userInfo["data"] as? String else {
            log.info("Data field not set")
            self.suppressNotifications()
            return
        }
        
        guard let dbUrl = getDatabasesDirectoryPath() else {
            log.error("Could not find databases directory")
            self.suppressNotifications()
            return
        }
        
        // If protected data is not yet available (e.g. device never unlocked after reboot),
        // show a minimal notification and skip DB access.
        if !protectedDataAvailable(at: dbUrl) {
            log.notice("Protected data unavailable; sending fallback notification")
            // Always remove any previously delivered "blocked" notifications to avoid duplicates
            clearProtectedBlockedNotifications()
            let fallback = UNMutableNotificationContent()
            fallback.categoryIdentifier = kProtectedBlockedCategory
            // TODO: This needs localization
            fallback.title = "Unlock your device"
            fallback.body = "You may have new messages, unlock your device to see them."
            fallback.sound = UNNotificationSound.default
            contentHandler(fallback)
            return
        }
        
        // Ensure any previously shown "blocked" notification is removed now that data is accessible
        clearProtectedBlockedNotifications()
        
        guard
            let sharedContainer = FileManager.default.containerURL(
                forSecurityApplicationGroupIdentifier: "group.ms.air")
        else {
            log.error("Could not find cache directory")
            self.suppressNotifications()
            return
        }
        let sharedCaches = sharedContainer.appendingPathComponent("Caches")
        let logFilePath = sharedCaches.appendingPathComponent("background.log").path
        log.info("Log file path: \(logFilePath, privacy: .public)")
        
        // Create IncomingNotificationContent object
        let incomingContent = IncomingNotificationContent(
            title: incomingNotification.title,
            body: incomingNotification.body,
            data: data,
            path: dbUrl.path,
            logFilePath: logFilePath
        )
        
        if let jsonData = try? JSONEncoder().encode(incomingContent),
           let jsonString = String(data: jsonData, encoding: .utf8)
        {
            
            jsonString.withCString { cString in
                guard let responsePointer = process_new_messages(cString) else {
                    log.error("process_new_messages returned nil")
                    self.suppressNotifications()
                    return
                }
                
                let responseString = String(cString: responsePointer)
                free_string(responsePointer)
                
                guard
                    let responseData = responseString.data(using: .utf8),
                    let notificationBatch = try? JSONDecoder().decode(
                        NotificationBatch.self, from: responseData)
                else {
                    log.error(
                        "Could not decode response from Rust: \(responseString, privacy: .public)")
                    self.suppressNotifications()
                    return
                }
                
                self.handleNotificationBatch(notificationBatch, contentHandler: contentHandler)
                log.info(
                    "Number of successfully processed messages: \(notificationBatch.additions.count)"
                )
            }
        } else {
            contentHandler(request.content)
        }
    }
    
    override func serviceExtensionTimeWillExpire() {
        log.notice("Expiration handler invoked")
        self.suppressNotifications()
    }
    
    func handleNotificationBatch(
        _ batch: NotificationBatch, contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let center = UNUserNotificationCenter.current()
        let dispatchGroup = DispatchGroup()
        
        // Remove notifications
        center.removeDeliveredNotifications(withIdentifiers: batch.removals)
        
        // When Rust does not return any notifications, we don't want to show anything
        if batch.additions.isEmpty {
            suppressNotifications(badge: batch.badgeCount)
            return
        }
        
        // Add notifications
        var lastNotification: NotificationContent?
        for (index, notificationContent) in batch.additions.enumerated() {
            // This cannot underflow because there is at least one addition
            if index == batch.additions.count - 1 {
                lastNotification = notificationContent
            } else {
                dispatchGroup.enter()
                let newContent = UNMutableNotificationContent()
                newContent.title = notificationContent.title
                newContent.body = notificationContent.body
                newContent.sound = UNNotificationSound.default
                if let chatId = notificationContent.chatId {
                    newContent.userInfo["chatId"] = chatId.uuid.uuidString
                }
                let request = UNNotificationRequest(
                    identifier: notificationContent.identifier.uuidString,
                    content: newContent,
                    trigger: nil)
                center.add(request) { error in
                    if let error = error {
                        log.error(
                            "Error adding notification: \(error.localizedDescription, privacy: .public)"
                        )
                    }
                    dispatchGroup.leave()
                }
            }
        }
        
        // Notify when all notifications are added
        dispatchGroup.notify(queue: DispatchQueue.main) {
            let content = UNMutableNotificationContent()
            if let lastNotification = lastNotification {
                content.title = lastNotification.title
                content.body = lastNotification.body
                content.sound = UNNotificationSound.default
                if let chatId = lastNotification.chatId {
                    content.userInfo["chatId"] = chatId.uuid.uuidString
                }
            }
            // Add the badge number
            content.badge = NSNumber(value: batch.badgeCount)
            // Delay the callback by 100 ms so that the notifications can be removed
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                contentHandler(content)
            }
        }
    }
    
    private func suppressNotifications(badge: UInt32? = nil) {
        guard let contentHandler = self.contentHandler else { return }
        let content = UNMutableNotificationContent()
        if let badge = badge {
            content.badge = NSNumber(value: badge)
        }
        contentHandler(content)
    }
    
    // Apply file protection
    private func applyProtection(_ url: URL) {
        let path = url.path
        try? FileManager.default.setAttributes(
            [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
            ofItemAtPath: path
        )
    }
    
    // Get a databases directory path that is NOT backed up to iCloud
    private func getDatabasesDirectoryPath() -> URL? {
        // Use the App Group container so extensions can also access it
        guard
            let containerURL = FileManager.default.containerURL(
                forSecurityApplicationGroupIdentifier: "group.ms.air"
            )
        else {
            return nil
        }
        
        // Prefer Library/Application Support for persistent, non-user‑visible data
        let dbsURL =
        containerURL
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent("Databases", isDirectory: true)
        
        do {
            try FileManager.default.createDirectory(at: dbsURL, withIntermediateDirectories: true)
            // exclude from backups
            var vals = URLResourceValues()
            vals.isExcludedFromBackup = true
            var u = dbsURL
            try? u.setResourceValues(vals)
            
            // enforce protection class
            applyProtection(dbsURL)
            
            return dbsURL
        } catch {
            return nil
        }
    }
    
    // Check if protected data is available
    func protectedDataAvailable(at dir: URL) -> Bool {
        let probe = dir.appendingPathComponent(".probe")
        // Try to read a byte or create+read; failures with EACCES/EPERM imply protected
        do {
            let _ = try Data(contentsOf: probe)  // or write Data() once at install time
            return true
        } catch let e as NSError {
            // NSCocoaErrorDomain Code=257 or NSPOSIXErrorDomain (1/13) commonly appear
            if e.domain == NSPOSIXErrorDomain, e.code == 1 || e.code == 13 { return false }
            if e.domain == NSCocoaErrorDomain, e.code == 257 { return false }  // no permission
            return true  // other errors (e.g., file not found) shouldn't block
        }
    }
    
    // Remove any delivered notifications that were shown due to protected data being unavailable
    private func clearProtectedBlockedNotifications() {
        let center = UNUserNotificationCenter.current()
        center.getDeliveredNotifications { notes in
            let ids =
            notes
                .filter { $0.request.content.categoryIdentifier == kProtectedBlockedCategory }
                .map { $0.request.identifier }
            if !ids.isEmpty {
                center.removeDeliveredNotifications(withIdentifiers: ids)
            }
        }
    }
}
