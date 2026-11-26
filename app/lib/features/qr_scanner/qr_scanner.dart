// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:camera/camera.dart';
import 'package:collection/collection.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:logging/logging.dart';

final _log = Logger('QrScanner');

/// Whether the QR scanner is offered on this platform.
bool get isQrScannerSupported =>
    defaultTargetPlatform == .android || defaultTargetPlatform == .iOS;

/// A camera that streams its frames to the QR-code scanner.
abstract interface class ScannerCamera {
  /// Builds the live camera feed with [child] on top of it.
  Widget buildPreview({required Widget child});

  /// Calls [onFrame] with the luma plane of every camera frame.
  Future<void> startFrames(ValueChanged<LumaFrame> onFrame);

  /// Focuses and meters on [point], relative to the preview.
  Future<void> focusOn(Offset point);

  /// Stops the frames and releases the camera.
  Future<void> dispose();
}

/// Opens a camera for the scanner, or returns null if the device has none.
typedef ScannerCameraOpener = Future<ScannerCamera?> Function();

/// Decodes the payload of a QR code in a camera frame, or returns null if the
/// frame holds none.
typedef QrFrameDecoder<T> = Future<T?> Function(
  BuildContext context,
  LumaFrame frame,
);

/// Opens the back camera, or the first camera if the device has no back
/// camera.
Future<ScannerCamera?> openScannerCamera() async {
  final cameras = await availableCameras();
  final description =
      cameras.firstWhereOrNull((camera) => camera.lensDirection == .back) ??
      cameras.firstOrNull;
  if (description == null) return null;

  final controller = CameraController(
    description,
    .medium,
    enableAudio: false,
    imageFormatGroup: .yuv420,
  );
  final camera = _ControllerCamera(controller);
  try {
    await controller.initialize();
  } on Object {
    _disposeCamera(camera);
    rethrow;
  }
  return camera;
}

class _ControllerCamera implements ScannerCamera {
  _ControllerCamera(this._controller);

  final CameraController _controller;

  @override
  Widget buildPreview({required Widget child}) =>
      CameraPreview(_controller, child: child);

  @override
  Future<void> startFrames(ValueChanged<LumaFrame> onFrame) =>
      _controller.startImageStream((image) {
        // With `ImageFormatGroup.yuv420` the first plane is the luma plane on
        // Android and iOS.
        final luma = image.planes.first;
        onFrame(
          LumaFrame(
            width: image.width,
            height: image.height,
            bytesPerRow: luma.bytesPerRow,
            bytes: luma.bytes,
          ),
        );
      });

  @override
  Future<void> focusOn(Offset point) async {
    if (_controller.value.focusPointSupported) {
      await _controller.setFocusPoint(point);
    }
    if (_controller.value.exposurePointSupported) {
      await _controller.setExposurePoint(point);
    }
  }

  @override
  Future<void> dispose() async {
    try {
      // Disposing the controller leaves its image stream subscribed.
      if (_controller.value.isStreamingImages) {
        await _controller.stopImageStream();
      }
    } finally {
      await _controller.dispose();
    }
  }
}

void _disposeCamera(ScannerCamera camera) {
  unawaited(
    camera.dispose().catchError((Object error, StackTrace stackTrace) {
      _log.warning('failed to release the camera', error, stackTrace);
    }),
  );
}

/// Why the scanner cannot use the camera.
enum _CameraProblem { permissionDenied, unavailable }

/// Scans a QR code with the camera.
///
/// Fills the available space with the camera feed and calls [onScanned] once,
/// with the first payload that [decode] returns. Frames that arrive while a
/// decode is running are dropped.
class QrScannerView<T extends Object> extends StatefulWidget {
  const QrScannerView({
    super.key,
    required this.openCamera,
    required this.decode,
    required this.onScanned,
    required this.manualEntryLabel,
    required this.onManualEntry,
  });

  final ScannerCameraOpener openCamera;
  final QrFrameDecoder<T> decode;
  final ValueChanged<T> onScanned;

  /// Label of the button that replaces the scanner when the camera cannot be
  /// used.
  final String manualEntryLabel;
  final VoidCallback onManualEntry;

  @override
  State<QrScannerView<T>> createState() => _QrScannerViewState<T>();
}

class _QrScannerViewState<T extends Object> extends State<QrScannerView<T>>
    with WidgetsBindingObserver {
  ScannerCamera? _camera;
  _CameraProblem? _problem;

  /// False while the app is not in the foreground.
  bool _foreground = true;
  bool _opening = false;
  bool _decoding = false;

  /// Set once a payload is found, to report only one.
  bool _handled = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    unawaited(_open());
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _release();
    super.dispose();
  }

  // The camera plugin leaves the camera running in the background. We release
  // it when the app leaves the foreground and open it again on return. A
  // problem is not retried, since on Android the retry would ask for a denied
  // permission again right away.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    switch (state) {
      case .resumed:
        _foreground = true;
        if (_problem == null) unawaited(_open());
      case .inactive || .hidden || .paused || .detached:
        _foreground = false;
        if (_camera != null) setState(_release);
    }
  }

  Future<void> _open() async {
    if (_opening || _camera != null || _handled) return;
    _opening = true;
    final ScannerCamera? camera;
    try {
      camera = await widget.openCamera();
    } on Object catch (error, stackTrace) {
      _log.warning('failed to open the camera', error, stackTrace);
      if (mounted) setState(() => _problem = _problemOf(error));
      return;
    } finally {
      _opening = false;
    }

    if (camera == null) {
      if (mounted) setState(() => _problem = .unavailable);
    } else if (!mounted || !_foreground || _handled) {
      _disposeCamera(camera);
    } else {
      setState(() => _camera = camera);
      await _startFrames(camera);
    }
  }

  Future<void> _startFrames(ScannerCamera camera) async {
    try {
      await camera.startFrames(_onFrame);
    } on Object catch (error, stackTrace) {
      _log.warning('failed to start the camera frames', error, stackTrace);
      if (mounted && _camera == camera) {
        setState(() {
          _release();
          _problem = .unavailable;
        });
      }
    }
  }

  void _release() {
    final camera = _camera;
    _camera = null;
    if (camera != null) _disposeCamera(camera);
  }

  Future<void> _onFrame(LumaFrame frame) async {
    if (_decoding || _handled || _camera == null || !mounted) return;
    _decoding = true;
    final stopwatch = Stopwatch()..start();
    T? payload;
    try {
      payload = await widget.decode(context, frame);
    } on Object catch (error, stackTrace) {
      _log.warning('failed to decode a camera frame', error, stackTrace);
    } finally {
      _decoding = false;
    }
    _log.fine(
      'decoding a ${frame.width}x${frame.height} frame with '
      '${frame.bytesPerRow} bytes per row took '
      '${stopwatch.elapsedMilliseconds} ms',
    );

    if (payload == null || _handled || !mounted) return;
    _handled = true;
    setState(_release);
    widget.onScanned(payload);
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final camera = _camera;

    return switch (_problem) {
      .permissionDenied => _ScannerMessage(
        message: loc.qrScanner_permissionDenied,
        manualEntryLabel: widget.manualEntryLabel,
        onManualEntry: widget.onManualEntry,
      ),
      .unavailable => _ScannerMessage(
        message: loc.qrScanner_cameraUnavailable,
        manualEntryLabel: widget.manualEntryLabel,
        onManualEntry: widget.onManualEntry,
      ),
      null when camera != null => _CameraFeed(camera: camera),
      null => const _ScannerPlaceholder(),
    };
  }
}

_CameraProblem _problemOf(Object error) => switch (error) {
  CameraException(
    code: 'CameraAccessDenied' ||
        'CameraAccessDeniedWithoutPrompt' ||
        'CameraAccessRestricted',
  ) =>
    .permissionDenied,
  _ => .unavailable,
};

/// The camera feed, cropped to fill the scanner.
class _CameraFeed extends StatelessWidget {
  const _CameraFeed({required this.camera});

  final ScannerCamera camera;

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: .expand,
      children: [
        // The preview takes the scanner's width at its own aspect ratio. The
        // fitted box then scales it to cover the scanner and clips the rest.
        LayoutBuilder(
          builder: (context, constraints) => FittedBox(
            fit: .cover,
            clipBehavior: .hardEdge,
            child: SizedBox(
              width: constraints.maxWidth,
              child: camera.buildPreview(child: _TapToFocus(camera: camera)),
            ),
          ),
        ),
        const IgnorePointer(child: _CornerBrackets()),
      ],
    );
  }
}

/// Focuses the camera on the tapped point of the preview.
class _TapToFocus extends StatelessWidget {
  const _TapToFocus({required this.camera});

  final ScannerCamera camera;

  Future<void> _focus(Offset point) async {
    try {
      await camera.focusOn(point);
    } on Object catch (error, stackTrace) {
      _log.warning('failed to focus the camera', error, stackTrace);
    }
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) => GestureDetector(
        behavior: .opaque,
        onTapUp: (details) => _focus(
          Offset(
            details.localPosition.dx / constraints.maxWidth,
            details.localPosition.dy / constraints.maxHeight,
          ),
        ),
      ),
    );
  }
}

/// Explains why the camera cannot be used and offers manual entry.
class _ScannerMessage extends StatelessWidget {
  const _ScannerMessage({
    required this.message,
    required this.manualEntryLabel,
    required this.onManualEntry,
  });

  final String message;
  final String manualEntryLabel;
  final VoidCallback onManualEntry;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return _CornerBrackets(
      child: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(S.s16),
          child: Column(
            mainAxisSize: .min,
            spacing: S.s16,
            children: [
              Text(
                message,
                textAlign: .center,
                style: typeScale.body.s.style(color: palette.text.secondary),
              ),
              Button(
                type: .secondary,
                label: manualEntryLabel,
                onPressed: onManualEntry,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Four rounded corner brackets framing the scan area, optionally wrapping a
/// [child] (e.g. the camera icon shown in the placeholder state).
class _CornerBrackets extends StatelessWidget {
  const _CornerBrackets({this.child});

  final Widget? child;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return Padding(
      padding: const EdgeInsets.all(S.s24),
      child: CustomPaint(
        painter: _CornerBracketsPainter(color: palette.text.quaternary),
        child: child ?? const SizedBox.expand(),
      ),
    );
  }
}

/// Shows the corner brackets around a camera icon while no camera feed is
/// live.
class _ScannerPlaceholder extends StatelessWidget {
  const _ScannerPlaceholder();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return _CornerBrackets(
      child: Center(
        child: AppIcon.camera(size: 32, color: palette.text.quaternary),
      ),
    );
  }
}

/// Paints four 90° corner brackets (with a small radius at the bend) just
/// inside the available bounds.
class _CornerBracketsPainter extends CustomPainter {
  const _CornerBracketsPainter({required this.color});

  final Color color;

  /// Length of each leg of a corner bracket.
  static const double _legLength = 20;

  /// Radius of the rounded bend joining the two legs.
  static const double _radius = 6;

  /// Stroke thickness of the brackets.
  static const double _strokeWidth = 2;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = .stroke
      ..strokeWidth = _strokeWidth
      ..strokeCap = .round;

    const double left = _strokeWidth / 2;
    const double top = _strokeWidth / 2;
    final double right = size.width - _strokeWidth / 2;
    final double bottom = size.height - _strokeWidth / 2;

    // Top-left.
    canvas.drawPath(
      Path()
        ..moveTo(left, top + _legLength)
        ..lineTo(left, top + _radius)
        ..arcToPoint(
          const Offset(left + _radius, top),
          radius: const Radius.circular(_radius),
        )
        ..lineTo(left + _legLength, top),
      paint,
    );

    // Top-right.
    canvas.drawPath(
      Path()
        ..moveTo(right - _legLength, top)
        ..lineTo(right - _radius, top)
        ..arcToPoint(
          Offset(right, top + _radius),
          radius: const Radius.circular(_radius),
        )
        ..lineTo(right, top + _legLength),
      paint,
    );

    // Bottom-right.
    canvas.drawPath(
      Path()
        ..moveTo(right, bottom - _legLength)
        ..lineTo(right, bottom - _radius)
        ..arcToPoint(
          Offset(right - _radius, bottom),
          radius: const Radius.circular(_radius),
        )
        ..lineTo(right - _legLength, bottom),
      paint,
    );

    // Bottom-left.
    canvas.drawPath(
      Path()
        ..moveTo(left + _legLength, bottom)
        ..lineTo(left + _radius, bottom)
        ..arcToPoint(
          Offset(left, bottom - _radius),
          radius: const Radius.circular(_radius),
        )
        ..lineTo(left, bottom - _legLength),
      paint,
    );
  }

  @override
  bool shouldRepaint(_CornerBracketsPainter oldDelegate) =>
      oldDelegate.color != color;
}
