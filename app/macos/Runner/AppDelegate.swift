import Cocoa
import FlutterMacOS
import UserNotifications

@main
class AppDelegate: FlutterAppDelegate, UNUserNotificationCenterDelegate {
  public static let notificationChannelName: String = "ms.air/channel"

  private var pendingInitialLink: String?
  // Whether Dart listens for opened links
  private var dartListeningForLinks = false

  override func applicationDidFinishLaunching(_ notification: Notification) {
    let center = UNUserNotificationCenter.current()
    center.delegate = self
    super.applicationDidFinishLaunching(notification)
  }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }

  // This method will be called when the user taps on the notification
  func userNotificationCenter(
    _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    NSLog("User opened notification")

    NSApp.activate(ignoringOtherApps: true)

    if let handle = NotificationHandle.init(notification: response.notification) {
      notifyFlutter(method: "openedNotification", arguments: handle.toDict())
    }

    completionHandler()
  }

  // A universal link, at launch or while the app runs
  override func application(
    _ application: NSApplication, continue userActivity: NSUserActivity,
    restorationHandler: @escaping ([NSUserActivityRestoring]) -> Void
  ) -> Bool {
    guard userActivity.activityType == NSUserActivityTypeBrowsingWeb,
      let url = userActivity.webpageURL
    else { return false }
    // Send the link to Dart, or keep it until Dart is ready
    if dartListeningForLinks {
      notifyFlutter(method: "openedLink", arguments: url.absoluteString)
    } else {
      pendingInitialLink = url.absoluteString
    }
    return true
  }

  // Dart listens for opened links after this call
  func takeInitialLink() -> String? {
    dartListeningForLinks = true
    defer { pendingInitialLink = nil }
    return pendingInitialLink
  }

  // Call Flutter by passing a method and customData as payload
  private func notifyFlutter(method: String, arguments: Any?) {
    let window = NSApplication.shared.windows.first as! MainFlutterWindow
    let controller = window.contentViewController as! FlutterViewController
    let channel = FlutterMethodChannel(
      name: Self.notificationChannelName, binaryMessenger: controller.engine.binaryMessenger)
    channel.invokeMethod(method, arguments: arguments)
  }
}
