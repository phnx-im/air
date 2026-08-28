// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import Flutter
import ImageIO
import Intents
import UIKit
import UniformTypeIdentifiers
import os

private let logger = Logger(subsystem: "ms.air.share", category: "ShareViewController")

private let shareChannelName = "ms.air/share"

// Matches ShareCubitBase::MAX_SHARED_ATTACHMENTS. One extra item is
// extracted so the Dart side still sees too many items and reports the
// too-many-attachments error.
private let maxAttachments = 10

// Upper bound for a single extracted file. Keeps oversized payloads from
// filling up the App Group container.
//
// The upload holds the plaintext and the ciphertext in memory, which an
// extension's memory budget does not survive for a large file. A file the
// server would reject anyway would take the extension down before the
// friendly error is shown. Tracks the deployed `max_attachment_size` (see
// StorageSettings in the backend) with slack.
private let maxAttachmentCopyBytes: UInt64 = 32 * 1024 * 1024

// Matches the limit the Rust side resizes images to
// (MAX_ATTACHMENT_IMAGE_WIDTH/HEIGHT in coreclient), preventing OOM
// issues in the share extension.
private let maxImagePixelSize = 2048

// Matches ATTACHMENT_IMAGE_QUALITY_PERCENT of the Rust re-encode.
private let downscaledImageQuality = 0.9

// Cache entries older than this are leftovers of a share session that was
// killed before its cleanup ran. They are removed on the next share.
private let staleCacheAge: TimeInterval = 24 * 60 * 60

/// Hosts the Flutter share UI inside the share extension process.
///
/// The extension runs its own Flutter engine on the `shareMain` entrypoint.
/// The Dart code and assets are loaded from the host app's `App.framework`
/// (resolved via `@executable_path/../../Frameworks`), so the appex itself
/// stays small. The user's database is opened from the App Group container.
/// Concurrent access with the main app is safe via WAL and the
/// inter-process file lock acquired by `CoreUser::load`.
class ShareViewController: UIViewController {
    private var flutterEngine: FlutterEngine?
    private var didComplete = false

    // Cache directories created for this share, removed again on close.
    // Written from concurrent item-provider callbacks.
    private let cacheDirsLock = NSLock()
    private var createdCacheDirs: [URL] = []

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        view.addGestureRecognizer(SheetDismissBlocker())
        deleteStaleCacheDirs()
        startFlutter()
    }

    private func startFlutter() {
        guard let project = hostAppDartProject() else {
            logger.error("Could not locate the host app's App.framework")
            close(success: false)
            return
        }

        let engine = FlutterEngine(name: "share", project: project)
        // The entrypoint is defined in main.dart (the default library).
        // Defining it only in the share module would leave it out of the
        // compiled program.
        let running = engine.run(withEntrypoint: "shareMain")
        guard running else {
            logger.error("Failed to run the Flutter engine")
            close(success: false)
            return
        }
        setupChannels(engine: engine)
        flutterEngine = engine

        let flutterViewController = FlutterViewController(
            engine: engine, nibName: nil, bundle: nil)
        addChild(flutterViewController)
        flutterViewController.view.frame = view.bounds
        flutterViewController.view.autoresizingMask = [
            .flexibleWidth, .flexibleHeight,
        ]
        view.addSubview(flutterViewController.view)
        flutterViewController.didMove(toParent: self)
    }

    // The Dart snapshot and the Flutter assets live in the host app's
    // App.framework. The extension bundle contains only this controller.
    private func hostAppDartProject() -> FlutterDartProject? {
        // <Runner.app>/PlugIns/ShareExtension.appex -> <Runner.app>
        let appBundleURL = Bundle.main.bundleURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let appFrameworkURL = appBundleURL.appendingPathComponent(
            "Frameworks/App.framework")
        guard let appFramework = Bundle(url: appFrameworkURL) else {
            logger.error(
                "App.framework not found at \(appFrameworkURL.path)")
            return nil
        }
        logger.info("Using Dart bundle at \(appFrameworkURL.path)")
        return FlutterDartProject(precompiledDartBundle: appFramework)
    }

    // MARK: - Method channels

    private func setupChannels(engine: FlutterEngine) {
        // Minimal subset of the main app channel used by the share engine
        let appChannel = FlutterMethodChannel(
            name: AppChannel.name, binaryMessenger: engine.binaryMessenger)
        appChannel.setMethodCallHandler { call, result in
            switch call.method {
            case "getDatabasesDirectory":
                if let url = AppGroup.databasesDirectory(create: false) {
                    result(url.path)
                } else {
                    result(
                        FlutterError(
                            code: "no_databases_directory",
                            message: "Databases directory not found",
                            details: nil))
                }
            case "getSharedCacheDirectory":
                if let url = AppGroup.sharedCachesDirectory(create: true) {
                    result(url.path)
                } else {
                    result(
                        FlutterError(
                            code: "no_shared_cache_directory",
                            message: "Shared cache directory not found",
                            details: nil))
                }
            default:
                result(FlutterMethodNotImplemented)
            }
        }

        let shareChannel = FlutterMethodChannel(
            name: shareChannelName, binaryMessenger: engine.binaryMessenger)
        shareChannel.setMethodCallHandler { [weak self] call, result in
            guard let self else {
                result(FlutterMethodNotImplemented)
                return
            }
            switch call.method {
            case "getSharedPayload":
                self.extractPayload(result: result)
            case "close":
                let success =
                    (call.arguments as? [String: Any])?["success"] as? Bool
                    ?? false
                result(nil)
                self.close(success: success)
            default:
                result(FlutterMethodNotImplemented)
            }
        }
    }

    // MARK: - Payload extraction

    // Extracts the shared items into files in the App Group caches and
    // hands `{text, attachments: [{path, mimeType}], shareTargetIdentifier}`
    // to Dart.
    private func extractPayload(result: @escaping FlutterResult) {
        let allProviders =
            (extensionContext?.inputItems as? [NSExtensionItem])?
            .flatMap { $0.attachments ?? [] } ?? []
        // Extract at most one item more than the share accepts, so the
        // too-many-attachments error still triggers without doing
        // unbounded work.
        let providers = Array(allProviders.prefix(maxAttachments + 1))

        // Conversation preselected via a donated INSendMessageIntent
        let shareTargetIdentifier =
            (extensionContext?.intent as? INSendMessageIntent)?
            .conversationIdentifier

        let group = DispatchGroup()
        let state = ExtractionState()

        for (index, provider) in providers.enumerated() {
            extractProvider(provider, at: index, into: state, group: group)
        }

        group.notify(queue: .main) {
            let texts = state.orderedTexts
            result([
                "text": texts.isEmpty
                    ? nil : texts.joined(separator: "\n\n"),
                "attachments": state.orderedAttachments,
                // Items whose representation could not be loaded or copied are
                // skipped above. The share UI says so instead of reporting a
                // silent success.
                "droppedAttachments": providers.count - state.extractedCount,
                "shareTargetIdentifier": shareTargetIdentifier,
            ])
        }
    }

    private func extractProvider(
        _ provider: NSItemProvider,
        at index: Int,
        into state: ExtractionState,
        group: DispatchGroup
    ) {
        // Order matters. Images and movies also conform to more generic
        // types.
        let fileTypes: [UTType] = [.image, .movie]
        for type in fileTypes
        where provider.hasItemConformingToTypeIdentifier(type.identifier) {
            loadFile(provider, at: index, type: type, into: state, group: group)
            return
        }

        // A shared file registers its content type next to `public.file-url`.
        // The file-url representation holds the URL rather than the file, and
        // lands in a temporary file named after the type, so the content type
        // is the one to load.
        if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier)
        {
            loadFile(
                provider, at: index, type: contentType(of: provider) ?? .data,
                into: state, group: group)
            return
        }

        if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
            group.enter()
            _ = provider.loadObject(ofClass: URL.self) { url, error in
                defer { group.leave() }
                guard let url else {
                    logger.error(
                        "Failed to load URL: \(error?.localizedDescription ?? "unknown error")"
                    )
                    return
                }
                state.addText(url.absoluteString, at: index)
            }
            return
        }

        if provider.hasItemConformingToTypeIdentifier(
            UTType.plainText.identifier)
        {
            group.enter()
            _ = provider.loadObject(ofClass: NSString.self) { text, error in
                defer { group.leave() }
                guard let text = text as? String else {
                    logger.error(
                        "Failed to load text: \(error?.localizedDescription ?? "unknown error")"
                    )
                    return
                }
                state.addText(text, at: index)
            }
            return
        }

        // Treat any remaining representation as a generic file
        if let identifier = provider.registeredTypeIdentifiers.first,
            let type = UTType(identifier)
        {
            loadFile(provider, at: index, type: type, into: state, group: group)
            return
        }

        logger.error("Shared item offers no usable representation")
    }

    // The most specific type a provider registers that is not a URL, which
    // for a shared file is the type of its content.
    private func contentType(of provider: NSItemProvider) -> UTType? {
        provider.registeredTypeIdentifiers
            .compactMap { UTType($0) }
            .first { !$0.conforms(to: .url) }
    }

    private func loadFile(
        _ provider: NSItemProvider,
        at index: Int,
        type: UTType,
        into state: ExtractionState,
        group: DispatchGroup
    ) {
        group.enter()
        provider.loadFileRepresentation(forTypeIdentifier: type.identifier) {
            [weak self] url, error in
            defer { group.leave() }
            guard let self, let url else {
                logger.error(
                    "Failed to load file representation: \(error?.localizedDescription ?? "unknown error")"
                )
                return
            }
            // The URL is only valid within this completion handler, so
            // copy the file into storage owned by the extension.
            let name = self.fileName(for: provider, loadedFrom: url)
            if type.conforms(to: .image),
                let target = self.downscaledImageCopy(url, named: name)
            {
                state.addAttachment(
                    path: target.path, mimeType: "image/jpeg", at: index)
                return
            }
            guard let target = self.copyToShareCache(url, named: name)
            else {
                return
            }
            let mimeType =
                type.preferredMIMEType
                ?? UTType(
                    filenameExtension: url.pathExtension
                )?.preferredMIMEType
            state.addAttachment(
                path: target.path, mimeType: mimeType, at: index)
        }
    }

    // The name the copy is stored under, which is the name the share UI shows
    // and the recipient receives. A provider that suggests one knows it better
    // than the temporary file the representation was written to, which is named
    // after the type where the provider suggests nothing.
    private func fileName(for provider: NSItemProvider, loadedFrom url: URL)
        -> String
    {
        // Only the last component, since a path separator in a suggested name
        // would put the copy outside the directory made for it.
        let suggested = (provider.suggestedName as NSString?)?.lastPathComponent
        guard let suggested, !suggested.isEmpty, suggested != ".",
            suggested != ".."
        else {
            return url.lastPathComponent
        }
        // A suggested name can come without an extension, which is what tells
        // the recipient how to open the file.
        if (suggested as NSString).pathExtension.isEmpty,
            !url.pathExtension.isEmpty
        {
            return "\(suggested).\(url.pathExtension)"
        }
        return suggested
    }

    // Copies an oversized still image into the App Group caches downscaled
    // to `maxImagePixelSize`, transcoded to JPEG. Returns nil where
    // downscaling does not apply. If the image is small enough, animated, or
    // not decodable, we leave the plain copy to handle the file.
    //
    // ImageIO scales images in streaming fashion, which prevents from running
    // out of memory in the share extension.
    private func downscaledImageCopy(_ url: URL, named name: String) -> URL? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
            // A multi-frame image is an animation, which a downscale to a
            // single frame would freeze. The Rust side re-encodes it with
            // its frames intact.
            CGImageSourceGetCount(source) == 1,
            let properties = CGImageSourceCopyPropertiesAtIndex(
                source, 0, nil) as? [CFString: Any],
            let width = properties[kCGImagePropertyPixelWidth] as? Int,
            let height = properties[kCGImagePropertyPixelHeight] as? Int,
            max(width, height) > maxImagePixelSize
        else {
            return nil
        }

        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            // Bakes the EXIF orientation into the pixels, since the
            // orientation tag does not survive the re-encode.
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxImagePixelSize,
        ]
        guard
            let scaled = CGImageSourceCreateThumbnailAtIndex(
                source, 0, options as CFDictionary)
        else {
            logger.error("Failed to downscale shared image")
            return nil
        }

        guard let shareCacheURL = shareCacheDirectory() else {
            return nil
        }
        let targetDir = shareCacheURL.appendingPathComponent(
            UUID().uuidString, isDirectory: true)
        let target = targetDir.appendingPathComponent(
            (name as NSString).deletingPathExtension + ".jpg")
        do {
            try FileManager.default.createDirectory(
                at: targetDir, withIntermediateDirectories: true)
        } catch {
            logger.error(
                "Failed to create image cache directory: \(error.localizedDescription)"
            )
            return nil
        }
        rememberCacheDir(targetDir)
        AppGroup.applyProtection(targetDir)

        guard
            let destination = CGImageDestinationCreateWithURL(
                target as CFURL, UTType.jpeg.identifier as CFString, 1, nil)
        else {
            logger.error("Failed to create image destination")
            return nil
        }
        let encodeOptions: [CFString: Any] = [
            kCGImageDestinationLossyCompressionQuality: downscaledImageQuality
        ]
        CGImageDestinationAddImage(
            destination, scaled, encodeOptions as CFDictionary)
        guard CGImageDestinationFinalize(destination) else {
            logger.error("Failed to write downscaled shared image")
            return nil
        }
        return target
    }

    // Copies the extracted file into the App Group caches, protected like
    // the databases directory.
    private func copyToShareCache(_ url: URL, named name: String) -> URL? {
        guard let shareCacheURL = shareCacheDirectory() else {
            return nil
        }
        if let size = try? FileManager.default
            .attributesOfItem(atPath: url.path)[.size] as? UInt64,
            size > maxAttachmentCopyBytes
        {
            logger.error("Shared file exceeds the copy limit, dropping it")
            return nil
        }
        let targetDir = shareCacheURL.appendingPathComponent(
            UUID().uuidString, isDirectory: true)
        let target = targetDir.appendingPathComponent(name)
        do {
            try FileManager.default.createDirectory(
                at: targetDir, withIntermediateDirectories: true)
            rememberCacheDir(targetDir)
            AppGroup.applyProtection(targetDir)
            try FileManager.default.copyItem(at: url, to: target)
            return target
        } catch {
            logger.error(
                "Failed to copy shared file: \(error.localizedDescription)")
            return nil
        }
    }

    private func shareCacheDirectory() -> URL? {
        AppGroup.sharedCachesDirectory(create: false)?
            .appendingPathComponent("share", isDirectory: true)
    }

    private func rememberCacheDir(_ url: URL) {
        cacheDirsLock.lock()
        defer { cacheDirsLock.unlock() }
        createdCacheDirs.append(url)
    }

    // Removes the cache directories created for this share. The attachment
    // content is persisted in the database at provision time, so the files
    // are not needed once the send finished (or was abandoned).
    private func removeCreatedCacheDirs() {
        cacheDirsLock.lock()
        let dirs = createdCacheDirs
        createdCacheDirs = []
        cacheDirsLock.unlock()
        for dir in dirs {
            try? FileManager.default.removeItem(at: dir)
        }
    }

    // Removes cache directories left behind by share sessions that were
    // killed before their cleanup ran.
    private func deleteStaleCacheDirs() {
        guard let shareCacheURL = shareCacheDirectory() else {
            return
        }
        DispatchQueue.global(qos: .utility).async {
            let cutoff = Date(timeIntervalSinceNow: -staleCacheAge)
            let entries =
                (try? FileManager.default.contentsOfDirectory(
                    at: shareCacheURL,
                    includingPropertiesForKeys: [.contentModificationDateKey]
                )) ?? []
            for entry in entries {
                let modified = (try? entry.resourceValues(
                    forKeys: [.contentModificationDateKey]
                ))?.contentModificationDate
                if let modified, modified < cutoff {
                    try? FileManager.default.removeItem(at: entry)
                }
            }
        }
    }

    // MARK: - Closing

    private func close(success: Bool) {
        if didComplete {
            return
        }
        didComplete = true
        removeCreatedCacheDirs()
        if success {
            // Wake a running main app so it reloads its stores (the send
            // path persisted store notifications into the database).
            CFNotificationCenterPostNotification(
                CFNotificationCenterGetDarwinNotifyCenter(),
                CFNotificationName(
                    AppChannel.storeNotificationsPendingName as CFString),
                nil, nil, true)
        }
        extensionContext?.completeRequest(returningItems: nil)
    }
}

// Turns off the sheet's drag-to-dismiss, so a drag reaches the Flutter view.
// See flutter/flutter#164670 for details.
private final class SheetDismissBlocker: UIGestureRecognizer,
    UIGestureRecognizerDelegate
{
    init() {
        super.init(target: nil, action: nil)
        delegate = self
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRequireFailureOf other: UIGestureRecognizer
    ) -> Bool {
        // Matched by name because the class is private to UIKit. A release
        // that renames it leaves the sheet dragging as it does today.
        if other.name?.hasSuffix(
            "UISheetInteractionBackgroundDismissRecognizer") == true
        {
            other.isEnabled = false
        }
        return false
    }

    // Ours only listens, so it leaves the arbitration it took part in rather
    // than sitting in `possible` and delaying the next touch sequence.
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .failed
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent)
    {
        state = .cancelled
    }
}

// Collects extraction results from concurrent item-provider callbacks.
//
// Callbacks complete in an arbitrary order, so results are keyed by the index
// of the provider they came from and assembled in that order. Otherwise a
// multi-photo share would arrive shuffled.
private final class ExtractionState {
    private let lock = NSLock()
    private var texts: [Int: String] = [:]
    private var attachments: [Int: [String: String?]] = [:]

    func addText(_ text: String, at index: Int) {
        lock.lock()
        defer { lock.unlock() }
        texts[index] = text
    }

    func addAttachment(path: String, mimeType: String?, at index: Int) {
        lock.lock()
        defer { lock.unlock() }
        attachments[index] = ["path": path, "mimeType": mimeType]
    }

    var orderedTexts: [String] {
        lock.lock()
        defer { lock.unlock() }
        return texts.sorted { $0.key < $1.key }.map { $0.value }
    }

    var orderedAttachments: [[String: String?]] {
        lock.lock()
        defer { lock.unlock() }
        return attachments.sorted { $0.key < $1.key }.map { $0.value }
    }

    // How many providers yielded something. A provider contributes at most one
    // result, so the rest were dropped.
    var extractedCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return texts.count + attachments.count
    }
}
