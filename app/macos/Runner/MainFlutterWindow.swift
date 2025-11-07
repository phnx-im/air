import Cocoa
import FlutterMacOS
import UserNotifications

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    let methodChannel = FlutterMethodChannel(
      name: AppDelegate.notificationChannelName,
      binaryMessenger: flutterViewController.engine.binaryMessenger)
    methodChannel.setMethodCallHandler(handleMethodCall)

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }

  private func handleMethodCall(call: FlutterMethodCall, result: @escaping FlutterResult) {
    if call.method == "sendNotification" {
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
    } else if call.method == "setBadgeCount" {
      if let args = call.arguments as? [String: Any?],
        let count = args["count"] as? Int
      {
        NSApp.dockTile.badgeLabel = count > 0 ? "\(count)" : nil
        result(nil)
      } else {
        result(
          FlutterError(
            code: "DecodingError",
            message: "Failed to decode setBadgeCount arguments",
            details: nil))
      }
    } else if call.method == "requestNotificationPermission" {
      requestNotificationPermission(result: result)
    } else {
      NSLog("Unknown method called: \(call.method)")
      result(FlutterMethodNotImplemented)
    }
  }

  private func requestNotificationPermission(result: @escaping FlutterResult) {
    let center = UNUserNotificationCenter.current()
    center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
      DispatchQueue.main.async {
        if let error = error {
          result(FlutterError(
            code: "PERMISSION_ERROR",
            message: "Failed to request notification permission: \(error.localizedDescription)",
            details: nil))
        } else {
          result(granted)
        }
      }
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
