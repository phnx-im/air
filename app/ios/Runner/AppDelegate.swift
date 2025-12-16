import Flutter
import UIKit

private let kProtectedBlockedCategory = "protected-blocked"

@main
@objc class AppDelegate: FlutterAppDelegate {
  private var deviceToken: String?
  private let notificationChannelName: String = "ms.air/channel"
  private var backgroundTaskId: UIBackgroundTaskIdentifier = .invalid

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    GeneratedPluginRegistrant.register(with: self)

    if #available(iOS 10.0, *) {
      UNUserNotificationCenter.current().delegate = self
    }

    // Register for push notifications
    UIApplication.shared.registerForRemoteNotifications()

    // Set up the method channel to retrieve the token from Flutter
    let controller = window?.rootViewController as! FlutterViewController
    let methodChannel = FlutterMethodChannel(
      name: notificationChannelName,
      binaryMessenger: controller.binaryMessenger)

    // Set the handler function for the method channel
    methodChannel.setMethodCallHandler(handleMethodCall)

    // Clear any lingering "blocked" notifications at launch
    clearProtectedBlockedNotifications()

    // When protected data becomes available (e.g. first unlock after reboot), clear again
    NotificationCenter.default.addObserver(
      self,
      selector: #selector(handleProtectedDataAvailable(_:)),
      name: UIApplication.protectedDataDidBecomeAvailableNotification,
      object: nil
    )

    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  override func application(
    _ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
  ) {
    NSLog("Device token available")
    let tokenParts = deviceToken.map { data in String(format: "%02.2hhx", data) }
    let token = tokenParts.joined()

    // Save the token in memory
    self.deviceToken = token
  }

  override func application(
    _ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error
  ) {
    NSLog("Failed to register: \(error)")
  }

  // This method will be called when app received push notifications in foreground
  override func userNotificationCenter(
    _ center: UNUserNotificationCenter, willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    NSLog("Foreground notification received")
    if let handle = NotificationHandle.init(notification: notification) {
      notifyFlutter(method: "receivedNotification", arguments: handle.toDict())
    }
    completionHandler([.alert, .sound])
  }

  // This method will be called when the user taps on the notification
  override func userNotificationCenter(
    _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    NSLog("User opened notification")
    if let handle = NotificationHandle.init(notification: response.notification) {
      notifyFlutter(method: "openedNotification", arguments: handle.toDict())
    }
    completionHandler()
  }

  override func applicationDidBecomeActive(_ application: UIApplication) {
    clearProtectedBlockedNotifications()
    super.applicationDidBecomeActive(application)
  }

  @objc private func handleProtectedDataAvailable(_ notification: Notification) {
    clearProtectedBlockedNotifications()
  }

  // Call Flutter by passing a method and customData as payload
  private func notifyFlutter(method: String, arguments: [String: Any?]) {
    let controller = window?.rootViewController as! FlutterViewController
    let channel = FlutterMethodChannel(
      name: notificationChannelName, binaryMessenger: controller.binaryMessenger)
    channel.invokeMethod(method, arguments: arguments)
  }

  // Define the handler function
  private func handleMethodCall(call: FlutterMethodCall, result: @escaping FlutterResult) {
    if call.method == "getDeviceToken" {
      self.getDeviceToken(result: result)
    } else if call.method == "getDatabasesDirectory" {
      if let path = self.getDatabasesDirectoryPath() {
        result(path)
      } else {
        result(
          FlutterError(
            code: "DIRECTORY_ERROR",
            message: "Failed to get databases directory path",
            details: nil
          ))
      }
    } else if call.method == "getSharedCacheDirectory" {
      if let path = self.getSharedCacheDirectory() {
        result(path)
      } else {
        result(
          FlutterError(
            code: "DIRECTORY_ERROR",
            message: "Failed to get shared cache directory path",
            details: nil
          ))
      }
    } else if call.method == "setBadgeCount" {
      if let args = call.arguments as? [String: Any], let count = args["count"] as? Int {
        self.setBadgeCount(count, result: result)
      } else {
        result(
          FlutterError(
            code: "INVALID_ARGUMENT", message: "Invalid or missing arguments", details: nil))
      }
    } else if call.method == "sendNotification" {
      if let args = call.arguments as? [String: Any?],
        let identifierStr = args["identifier"] as? String,
        let identifier = UUID(uuidString: identifierStr),
        let title = args["title"] as? String,
        let body = args["body"] as? String,
        let chatIdStr = args["chatId"] as? String?
      {
        sendNotification(
          identifier: identifier,
          title: title,
          body: body,
          chatId: chatIdStr.flatMap { UUID(uuidString: $0) })
        result(nil)
      } else {
        result(
          FlutterError(
            code: "DecodingError",
            message: "Failed to decode sendNotifications arguments",
            details: nil))
      }
    } else if call.method == "getActiveNotifications" {
      getActiveNotifications { handles in
        result(handles.map { $0.toDict() })
      }
    } else if call.method == "cancelNotifications" {
      if let args = call.arguments as? [String: Any?],
        let identifiers = args["identifiers"] as? [String]
      {
        let ids = identifiers.compactMap { UUID(uuidString: $0) }
        cancelNotifications(identifiers: ids)
        result(nil)
      } else {
        result(
          FlutterError(
            code: "DecodingError",
            message: "Failed to decode cancelNotifications arguments",
            details: nil))
      }
    } else if call.method == "beginBackgroundTask" {
      let taskId = self.beginBackgroundTask()
      result(Int(taskId.rawValue))
    } else if call.method == "endBackgroundTask" {
      if let args = call.arguments as? [String: Any],
         let rawId = args["taskId"] as? Int {
        self.endBackgroundTask(taskId: UIBackgroundTaskIdentifier(rawValue: rawId))
      }
      result(nil)
    } else {
      NSLog("Unknown method called: \(call.method)")
      result(FlutterMethodNotImplemented)
    }

  }

  // Get device token
  private func getDeviceToken(result: FlutterResult) {
    if let token = deviceToken {
      result(token)
    } else {
      result(FlutterError(code: "UNAVAILABLE", message: "Device token not available", details: nil))
    }
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
  private func getDatabasesDirectoryPath() -> String? {
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
      try createBackupExcludedDirectory(at: dbsURL)
      return dbsURL.path
    } catch {
      return nil
    }
  }

  // Get a cache directory path that is shared between the application and the
  // background extension
  private func getSharedCacheDirectory() -> String? {
    guard
      let sharedContainer = FileManager.default.containerURL(
        forSecurityApplicationGroupIdentifier: "group.ms.air"
      )
    else {
      return nil
    }

    let sharedCaches = sharedContainer.appendingPathComponent("Caches")

    do {
      try createBackupExcludedDirectory(at: sharedCaches)
      return sharedCaches.path
    } catch {
      return nil
    }
  }

  private func createBackupExcludedDirectory(at url: URL) throws {
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    // exclude from backups
    var vals = URLResourceValues()
    vals.isExcludedFromBackup = true
    var u = url
    try? u.setResourceValues(vals)

    // enforce protection class
    applyProtection(url)
  }

  // Set the badge count
  private func setBadgeCount(_ count: Int, result: FlutterResult) {
    UIApplication.shared.applicationIconBadgeNumber = count
    result(nil)
  }

  private func beginBackgroundTask() -> UIBackgroundTaskIdentifier {
    if backgroundTaskId != .invalid {
      return backgroundTaskId
    }
    backgroundTaskId = UIApplication.shared.beginBackgroundTask(
      withName: "prepareForBackground",
      expirationHandler: { [weak self] in
        guard let self else { return }
        // Notify Flutter so it can log expiration.
        notifyFlutter(
          method: "backgroundTaskExpired",
          arguments: ["taskId": Int(self.backgroundTaskId.rawValue)]
        )
        if self.backgroundTaskId != .invalid {
          UIApplication.shared.endBackgroundTask(self.backgroundTaskId)
          self.backgroundTaskId = .invalid
        }
      })
    return backgroundTaskId
  }

  private func endBackgroundTask(taskId: UIBackgroundTaskIdentifier) {
    if taskId != .invalid {
      UIApplication.shared.endBackgroundTask(taskId)
    }
    if taskId == backgroundTaskId {
      backgroundTaskId = .invalid
    }
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

func sendNotification(identifier: UUID, title: String, body: String, chatId: UUID?) {
  let center = UNUserNotificationCenter.current()

  let content = UNMutableNotificationContent()
  content.title = title
  content.body = body
  content.sound = UNNotificationSound.default
  content.userInfo["chatId"] = chatId?.uuidString

  let request = UNNotificationRequest(
    identifier: identifier.uuidString,
    content: content,
    trigger: nil)

  center.add(request) { error in
    if let error = error {
      NSLog("NSE Error adding notification: \(error)")
    }
  }
}

struct NotificationHandle {
  let identifier: UUID
  let chatId: UUID?

  init?(notification: UNNotification) {
    let identifierStr = notification.request.identifier
    guard let identifier = UUID(uuidString: identifierStr) else {
      return nil
    }
    self.identifier = identifier
    let chatIdStr: String? =
      notification.request.content.userInfo["chatId"] as? String? ?? nil
    self.chatId = chatIdStr.flatMap { UUID(uuidString: $0) }
  }

  func toDict() -> [String: Any?] {
    [
      "identifier": identifier.uuidString,
      "chatId": chatId?.uuidString,
    ]
  }
}

func getActiveNotifications(completionHandler: @escaping ([NotificationHandle]) -> Void) {
  let center = UNUserNotificationCenter.current()
  center.getDeliveredNotifications { notifications in
    completionHandler(
      notifications.compactMap {
        NotificationHandle(notification: $0)
      })
  }
}

func cancelNotifications(identifiers: [UUID]) {
  let center = UNUserNotificationCenter.current()
  center.removeDeliveredNotifications(
    withIdentifiers: identifiers.map {
      $0.uuidString
    })
}
