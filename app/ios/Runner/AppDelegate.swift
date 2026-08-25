import Flutter
import Intents
import UIKit

private let kProtectedBlockedCategory = "protected-blocked"

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
    private var deviceToken: String?
    private var pendingAdmissionChallenge: PendingAdmissionChallenge?
    private let notificationChannelName: String = AppChannel.name
    private var backgroundTaskId: UIBackgroundTaskIdentifier = .invalid
    private var storeNotificationsChannel: FlutterMethodChannel?

    override func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication
            .LaunchOptionsKey: Any]?
    ) -> Bool {
        if #available(iOS 10.0, *) {
            UNUserNotificationCenter.current().delegate = self
        }

        // Register for push notifications
        UIApplication.shared.registerForRemoteNotifications()

        // Clear any lingering "blocked" notifications at launch
        clearProtectedBlockedNotifications()

        // When protected data becomes available (e.g. first unlock after reboot), clear again
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleProtectedDataAvailable(_:)),
            name: UIApplication.protectedDataDidBecomeAvailableNotification,
            object: nil
        )

        return super.application(
            application, didFinishLaunchingWithOptions: launchOptions)
    }

    func didInitializeImplicitFlutterEngine(
        _ engineBridge: FlutterImplicitEngineBridge
    ) {
        GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)

        // Set up the method channel to retrieve the token from Flutter
        let methodChannel = FlutterMethodChannel(
            name: notificationChannelName,
            binaryMessenger: engineBridge.applicationRegistrar.messenger())

        // Set the handler function for the method channel
        methodChannel.setMethodCallHandler(handleMethodCall)

        storeNotificationsChannel = methodChannel
        let observer = Unmanaged.passUnretained(self).toOpaque()
        CFNotificationCenterAddObserver(
            CFNotificationCenterGetDarwinNotifyCenter(),
            observer,
            { (_, observer, _, _, _) in
                guard let observer = observer else { return }
                let appDelegate = Unmanaged<AppDelegate>
                    .fromOpaque(observer).takeUnretainedValue()
                // Fire away on the main channel, since that's where Flutter will listen
                DispatchQueue.main.async {
                    appDelegate.storeNotificationsChannel?.invokeMethod(
                        "processStoreNotifications", arguments: nil)
                }
            },
            AppChannel.storeNotificationsPendingName as CFString,
            nil,
            .deliverImmediately)
    }

    override func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        NSLog("Device token available")
        let tokenParts = deviceToken.map { data in
            String(format: "%02.2hhx", data)
        }
        let token = tokenParts.joined()

        // Save the token in memory
        self.deviceToken = token
    }

    override func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        NSLog("Failed to register: \(error)")
    }

    // FlutterAppDelegate answers respondsToSelector: for this selector out of
    // its plugin registry alone and never falls through to super, so an
    // implementation in a subclass stays invisible to UIKit unless some plugin
    // also claims it. We have no push plugin, so answer for ourselves.
    private static let remoteNotificationSelector = Selector(
        "application:didReceiveRemoteNotification:fetchCompletionHandler:")

    override func responds(to aSelector: Selector!) -> Bool {
        if aSelector == Self.remoteNotificationSelector {
            return true
        }
        return super.responds(to: aSelector)
    }

    // A silent push carrying an admission challenge.
    override func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any],
        fetchCompletionHandler completionHandler: @escaping (
            UIBackgroundFetchResult
        ) -> Void
    ) {
        guard let challenge = userInfo["challenge"] as? String,
            let sessionId = userInfo["sessionId"] as? String
        else {
            // Not a challenge. FlutterAppDelegate has no implementation of this
            // selector to defer to, so complete the fetch here.
            completionHandler(.noData)
            return
        }

        NSLog("Admission challenge received for session \(sessionId)")
        // We deliver it, but also keep it in memory so Flutter can retrieve it
        // if it wasn't ready to receive it yet.
        let pending = PendingAdmissionChallenge(challenge: challenge, sessionId: sessionId)
        pendingAdmissionChallenge = pending
        storeNotificationsChannel?.invokeMethod(
            "receivedAdmissionChallenge", arguments: pending.channelArguments)
        completionHandler(.newData)
    }

    // This method will be called when app received push notifications in foreground
    override func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler:
            @escaping (
                UNNotificationPresentationOptions
            ) -> Void
    ) {
        NSLog("Foreground notification received")
        if let handle = NotificationHandle.init(notification: notification) {
            notifyFlutter(
                method: "receivedNotification", arguments: handle.toDict())
        }
        completionHandler([.alert, .sound])
    }

    // This method will be called when the user taps on the notification
    override func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        NSLog("User opened notification")
        // Dismiss any presented view controller (e.g. native image picker) so it
        // doesn't stay on top of the chat we're about to navigate to.
        if let presented = window?.rootViewController?.presentedViewController {
            presented.dismiss(animated: false)
        }
        if let handle = NotificationHandle.init(
            notification: response.notification)
        {
            notifyFlutter(
                method: "openedNotification", arguments: handle.toDict())
        }
        completionHandler()
    }

    override func applicationDidBecomeActive(_ application: UIApplication) {
        clearProtectedBlockedNotifications()
        super.applicationDidBecomeActive(application)
    }

    @objc private func handleProtectedDataAvailable(
        _ notification: Notification
    ) {
        clearProtectedBlockedNotifications()
    }

    // Call Flutter by passing a method and customData as payload
    private func notifyFlutter(method: String, arguments: [String: Any?]) {
        storeNotificationsChannel?.invokeMethod(method, arguments: arguments)
    }

    // Define the handler function
    private func handleMethodCall(
        call: FlutterMethodCall, result: @escaping FlutterResult
    ) {
        if call.method == "getDeviceToken" {
            self.getDeviceToken(result: result)
        } else if call.method == "getPendingAdmissionChallenge" {
            result(self.takePendingAdmissionChallenge())
        } else if call.method == "getDatabasesDirectory" {
            if let url = AppGroup.databasesDirectory(create: true) {
                result(url.path)
            } else {
                result(
                    FlutterError(
                        code: "DIRECTORY_ERROR",
                        message: "Failed to get databases directory path",
                        details: nil
                    ))
            }
        } else if call.method == "getSharedCacheDirectory" {
            if let url = AppGroup.sharedCachesDirectory(create: true) {
                result(url.path)
            } else {
                result(
                    FlutterError(
                        code: "DIRECTORY_ERROR",
                        message: "Failed to get shared cache directory path",
                        details: nil
                    ))
            }
        } else if call.method == "setBadgeCount" {
            if let args = call.arguments as? [String: Any],
                let count = args["count"] as? Int
            {
                self.setBadgeCount(count, result: result)
            } else {
                result(
                    FlutterError(
                        code: "INVALID_ARGUMENT",
                        message: "Invalid or missing arguments", details: nil))
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
                        message:
                            "Failed to decode cancelNotifications arguments",
                        details: nil))
            }
        } else if call.method == "getClipboardImage" {
            let pasteboard = UIPasteboard.general
            // Animated formats and formats image-rs supports are passed to Rust
            // directly.
            let preferred: [(uti: String, mime: String)] = [
                ("com.compuserve.gif", "image/gif"),
                ("org.webmproject.webp", "image/webp"),
                ("public.png", "image/png"),
                ("public.jpeg", "image/jpeg"),
            ]
            var payload: [String: Any]?
            for (uti, mime) in preferred {
                if let data = pasteboard.data(forPasteboardType: uti) {
                    payload = [
                        "bytes": FlutterStandardTypedData(bytes: data),
                        "mimeType": mime,
                    ]
                    break
                }
            }
            // Fallback for HEIC and other formats UIImage decodes but the
            // Rust image crate does not.
            if payload == nil,
                let image = pasteboard.image,
                let png = image.pngData()
            {
                payload = [
                    "bytes": FlutterStandardTypedData(bytes: png),
                    "mimeType": "image/png",
                ]
            }
            result(payload)
        } else if call.method == "beginBackgroundTask" {
            let taskId = self.beginBackgroundTask()
            result(Int(taskId.rawValue))
        } else if call.method == "endBackgroundTask" {
            if let args = call.arguments as? [String: Any],
                let rawId = args["taskId"] as? Int
            {
                self.endBackgroundTask(
                    taskId: UIBackgroundTaskIdentifier(rawValue: rawId))
            }
            result(nil)
        } else if call.method == "requestNotificationPermission" {
            requestNotificationPermission(result: result)
        } else if call.method == "donateShareTarget" {
            donateShareTarget(call: call, result: result)
        } else if call.method == "clearShareTargets" {
            INInteraction.deleteAll { error in
                if let error {
                    NSLog("Failed to delete intent donations: \(error)")
                }
            }
            result(nil)
        } else {
            NSLog("Unknown method called: \(call.method)")
            result(FlutterMethodNotImplemented)
        }

    }

    // Donates an INSendMessageIntent for the chat so it appears as a direct
    // target in the system share sheet (handled by the share extension).
    private func donateShareTarget(
        call: FlutterMethodCall, result: @escaping FlutterResult
    ) {
        // The INSendMessageIntent initializer used here requires iOS 14.
        // Older versions donate no share targets.
        guard #available(iOS 14.0, *) else {
            result(nil)
            return
        }
        guard let args = call.arguments as? [String: Any],
            let chatId = args["chatId"] as? String,
            let title = args["title"] as? String
        else {
            result(
                FlutterError(
                    code: "INVALID_ARGUMENTS",
                    message: "chatId or title not provided",
                    details: nil
                ))
            return
        }
        let isGroup = args["isGroup"] as? Bool ?? false
        let pictureData = (args["picture"] as? FlutterStandardTypedData)?.data
        let image = pictureData.map { INImage(imageData: $0) }

        let recipient = INPerson(
            personHandle: INPersonHandle(value: chatId, type: .unknown),
            nameComponents: nil,
            displayName: title,
            image: image,
            contactIdentifier: nil,
            customIdentifier: chatId
        )
        let intent = INSendMessageIntent(
            recipients: [recipient],
            outgoingMessageType: .outgoingMessageText,
            content: nil,
            speakableGroupName: isGroup
                ? INSpeakableString(spokenPhrase: title) : nil,
            conversationIdentifier: chatId,
            serviceName: nil,
            sender: nil,
            attachments: nil
        )
        if isGroup, let image {
            intent.setImage(image, forParameterNamed: \.speakableGroupName)
        }

        let interaction = INInteraction(intent: intent, response: nil)
        interaction.direction = .outgoing
        interaction.donate { error in
            if let error {
                NSLog("Failed to donate share target intent: \(error)")
            }
        }
        result(nil)
    }

    // Get device token
    private func getDeviceToken(result: FlutterResult) {
        result(deviceToken)
    }

    private func takePendingAdmissionChallenge() -> [String: String]? {
        defer { pendingAdmissionChallenge = nil }
        guard let pending = pendingAdmissionChallenge else { return nil }
        return pending.channelArguments
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
                    UIApplication.shared.endBackgroundTask(
                        self.backgroundTaskId)
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

    private func requestNotificationPermission(
        result: @escaping FlutterResult
    ) {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            switch settings.authorizationStatus {
            case .notDetermined:
                center.requestAuthorization(options: [.alert, .sound, .badge]) {
                    granted, error in
                    DispatchQueue.main.async {
                        if let error = error {
                            result(
                                FlutterError(
                                    code: "PERMISSION_ERROR",
                                    message:
                                        "Failed to request notification permission: \(error.localizedDescription)",
                                    details: nil))
                        } else {
                            result(granted)
                        }
                    }
                }
            case .authorized, .provisional, .ephemeral:
                DispatchQueue.main.async { result(true) }
            case .denied:
                DispatchQueue.main.async { result(false) }
            @unknown default:
                DispatchQueue.main.async { result(false) }
            }
        }
    }
}

// Remove any delivered notifications that were shown due to protected data being unavailable
private func clearProtectedBlockedNotifications() {
    let center = UNUserNotificationCenter.current()
    center.getDeliveredNotifications { notes in
        let ids =
            notes
            .filter {
                $0.request.content.categoryIdentifier
                    == kProtectedBlockedCategory
            }
            .map { $0.request.identifier }
        if !ids.isEmpty {
            center.removeDeliveredNotifications(withIdentifiers: ids)
        }
    }
}

func sendNotification(
    identifier: UUID, title: String, body: String, chatId: UUID?
) {
    let center = UNUserNotificationCenter.current()

    let content = UNMutableNotificationContent()
    content.title = title
    content.body = body
    content.sound = UNNotificationSound.default
    content.userInfo["chatId"] = chatId?.uuidString
    // Group the per-message notifications by chat in the notification center
    if let chatId = chatId {
        content.threadIdentifier = chatId.uuidString
    }

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

func getActiveNotifications(
    completionHandler: @escaping ([NotificationHandle]) -> Void
) {
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

struct PendingAdmissionChallenge {
    let challenge: String
    let sessionId: String

    var channelArguments: [String: String] {
        ["sessionId": sessionId, "challenge": challenge]
    }
}
