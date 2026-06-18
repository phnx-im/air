// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/components/modal/app_dialog.dart';
import 'package:air/ds/components/modal/confirm_dialog.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:air/user/user_cubit.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

bool get _isQrCodeScannerSupported =>
    defaultTargetPlatform == .android || defaultTargetPlatform == .iOS;

String initialDeviceName(TargetPlatform platform) => switch (platform) {
  TargetPlatform.android => "Android",
  TargetPlatform.iOS => "iOS",
  TargetPlatform.linux => "Linux",
  TargetPlatform.macOS => "macOS",
  TargetPlatform.windows => "Windows",
  _ => "New device",
};

enum _LinkPage { chooser, scanQrCode, numericCode, linking }

/// A running linking session
typedef LinkSession = ({
  Stream<MultiDeviceLinkEvent> events,
  VoidCallback confirm,
});

/// Starts a linking session for [sessionId]. Injectable for tests.
typedef LinkSessionStarter =
    LinkSession Function(BuildContext context, String sessionId);

LinkSession _startLinkSession(BuildContext context, String sessionId) {
  final confirmation = MultiDeviceLinkConfirmation();
  final events = context.read<UserCubit>().linkDevice(sessionId, confirmation);
  return (events: events, confirm: confirmation.confirm);
}

/// Entry point for linking a new device.
class LinkDeviceModal extends HookWidget {
  const LinkDeviceModal({super.key, this.startLinkSession = _startLinkSession});

  /// Starts the linking session for the [_LinkPage.linking] page.
  final LinkSessionStarter startLinkSession;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final page = useState(_LinkPage.chooser);
    // The linking code being driven on the [_LinkPage.linking] page.
    final sessionId = useState<String?>(null);

    void backToChooser() => page.value = _LinkPage.chooser;

    return AppDialog(
      maxWidth: 500,
      backgroundColor: colors.backgroundBase.quaternary,
      child: switch (page.value) {
        _LinkPage.chooser => _LinkChooserPage(
          onScanQrCode: () => page.value = _LinkPage.scanQrCode,
          onEnterNumericCode: () => page.value = _LinkPage.numericCode,
          onClose: () => Navigator.of(context).pop(),
        ),
        _LinkPage.scanQrCode => _ScanQrCodePage(
          onBack: backToChooser,
          onCodeScanned: (code) {
            sessionId.value = code;
            page.value = _LinkPage.linking;
          },
        ),
        _LinkPage.numericCode => _NumericCodePage(
          onBack: backToChooser,
          onSubmit: (code) {
            sessionId.value = code;
            page.value = _LinkPage.linking;
          },
        ),
        _LinkPage.linking => _LinkingPage(
          sessionId: sessionId.value!,
          onBack: backToChooser,
          startLinkSession: startLinkSession,
        ),
      },
    );
  }
}

class _LinkModalHeader extends StatelessWidget {
  const _LinkModalHeader({required this.title, this.onBack});

  final String title;
  final VoidCallback? onBack;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Stack(
      alignment: Alignment.center,
      children: [
        if (onBack != null)
          Align(
            alignment: Alignment.centerLeft,
            child: GlassCircleButton(
              icon: AppIcon.arrowLeft(size: 20, color: colors.text.primary),
              color: colors.accent.quaternary,
              onPressed: onBack,
            ),
          ),
        Text(
          title,
          style: TextStyle(
            fontSize: BodyFontSize.base.size,
            fontWeight: FontWeight.bold,
          ),
        ),
      ],
    );
  }
}

/// Explains the flow and offers the two linking methods.
class _LinkChooserPage extends StatelessWidget {
  const _LinkChooserPage({
    required this.onScanQrCode,
    required this.onEnterNumericCode,
    required this.onClose,
  });

  final VoidCallback onScanQrCode;
  final VoidCallback onEnterNumericCode;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    final labelStyle = TextStyle(
      color: colors.text.secondary,
      fontSize: LabelFontSize.small1.size,
    );

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _LinkModalHeader(
          title: loc.linkedDevicesScreen_linkDevice,
          onBack: onClose,
        ),
        const SizedBox(height: Spacing.px24),
        Text(
          loc.linkedDevicesScreen_linkDialog_chooseMethod,
          style: labelStyle,
        ),
        const SizedBox(height: Spacing.px16),
        Text(loc.linkedDevicesScreen_linkDialog_openApp, style: labelStyle),
        const SizedBox(height: Spacing.px24),
        AppButton(
          type: .secondary,
          label: _isQrCodeScannerSupported
              ? loc.linkedDevicesScreen_linkDialog_scanQrCode
              : loc.linkedDevicesScreen_linkDialog_scanQrCode_unavailable,
          state: _isQrCodeScannerSupported ? .active : .inactive,
          icon: (size, color) => AppIcon.qrCode(size: size.width, color: color),
          alignment: .start,
          onPressed: onScanQrCode,
        ),
        const SizedBox(height: Spacing.px12),
        AppButton(
          type: .secondary,
          label: loc.linkedDevicesScreen_linkDialog_enterNumericCode,
          icon: (size, color) => AppIcon.tag(size: size.width, color: color),
          alignment: .start,
          onPressed: onEnterNumericCode,
        ),
        const SizedBox(height: Spacing.px16),
        Text(
          loc.linkedDevicesScreen_linkDialog_warning,
          style: TextStyle(
            color: colors.function.danger,
            fontSize: BodyFontSize.small2.size,
          ),
        ),
      ],
    );
  }
}

/// Second page: the QR-code scanner.
class _ScanQrCodePage extends StatefulWidget {
  const _ScanQrCodePage({required this.onBack, required this.onCodeScanned});

  final VoidCallback onBack;
  final ValueChanged<String> onCodeScanned;

  @override
  State<_ScanQrCodePage> createState() => _ScanQrCodePageState();
}

class _ScanQrCodePageState extends State<_ScanQrCodePage> {
  /// Latched once a linking code is found, so we stop probing scanned codes
  /// (the scanner keeps firing until the page is swapped out).
  bool _handled = false;

  void _onDetect(BarcodeCapture capture) {
    if (_handled) return;

    final userCubit = context.read<UserCubit>();
    for (final raw in capture.barcodes.map((barcode) => barcode.rawValue)) {
      if (raw == null) continue;
      final sessionId = userCubit.parseLinkingUrl(raw);
      if (sessionId != null) {
        _handled = true;
        widget.onCodeScanned(sessionId);
        return;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _LinkModalHeader(
          title: loc.linkedDevicesScreen_linkDialog_scanQrCode,
          onBack: widget.onBack,
        ),
        const SizedBox(height: Spacing.px24),
        AspectRatio(
          aspectRatio: 1,
          child: ClipRRect(
            borderRadius: BorderRadius.circular(Spacing.px16),
            child: ColoredBox(
              color: colors.backgroundBase.secondary,
              child: _isQrCodeScannerSupported
                  ? MobileScanner(
                      tapToFocus: true,
                      placeholderBuilder: (_) => const _ScannerPlaceholder(),
                      overlayBuilder: (_, _) => const _CornerBrackets(),
                      onDetect: _onDetect,
                    )
                  : Center(
                      child: AppIcon.qrCode(
                        size: 64,
                        color: colors.text.quaternary,
                      ),
                    ),
            ),
          ),
        ),
      ],
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
    final colors = CustomColorScheme.of(context);

    return Padding(
      padding: const EdgeInsets.all(Spacing.px24),
      child: CustomPaint(
        painter: _CornerBracketsPainter(color: colors.text.quaternary),
        child: child ?? const SizedBox.expand(),
      ),
    );
  }
}

/// Shown while the camera is initializing: the corner brackets framing a
/// centered camera icon. Once the feed is live the icon is gone and only the
/// brackets remain (see [_CornerBrackets] used as the scanner overlay).
class _ScannerPlaceholder extends StatelessWidget {
  const _ScannerPlaceholder();

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return _CornerBrackets(
      child: Center(
        child: AppIcon.camera(size: 32, color: colors.text.quaternary),
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
      ..style = PaintingStyle.stroke
      ..strokeWidth = _strokeWidth
      ..strokeCap = StrokeCap.round;

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

/// Third page: enter the numeric linking code shown on the new device.
class _NumericCodePage extends HookWidget {
  const _NumericCodePage({required this.onBack, required this.onSubmit});

  final VoidCallback onBack;

  /// Called with the entered linking code when the user taps "Link".
  final ValueChanged<String> onSubmit;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);
    final controller = useTextEditingController();

    final codeStyle = TextStyle(
      fontSize: HeaderFontSize.h1.size,
      fontWeight: FontWeight.bold,
      fontFeatures: const [FontFeature.tabularFigures()],
      letterSpacing: 4,
      color: colors.text.primary,
    );

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _LinkModalHeader(
          title: loc.linkedDevicesScreen_linkDialog_enterNumericCode,
          onBack: onBack,
        ),
        const SizedBox(height: Spacing.px24),
        Text(
          loc.linkedDevicesScreen_linkDialog_numericCodeInstructions,
          style: TextStyle(
            color: colors.text.secondary,
            fontSize: LabelFontSize.small1.size,
          ),
        ),
        const SizedBox(height: Spacing.px16),
        TextField(
          controller: controller,
          autofocus: true,
          keyboardType: TextInputType.number,
          textAlign: TextAlign.center,
          inputFormatters: [FilteringTextInputFormatter.digitsOnly],
          buildCounter:
              (_, {required currentLength, required isFocused, maxLength}) =>
                  null,
          style: codeStyle,
          decoration: appDialogInputDecoration.copyWith(
            filled: true,
            fillColor: colors.backgroundBase.secondary,
            contentPadding: const EdgeInsets.symmetric(
              vertical: Spacing.px24,
              horizontal: Spacing.px8,
            ),
            hintText: "0000 0000",
            hintStyle: codeStyle.copyWith(color: colors.text.quaternary),
          ),
        ),
        const SizedBox(height: Spacing.px24),
        AppButton(
          type: .primary,
          label: loc.linkedDevicesScreen_linkDialog_link,
          onPressed: () {
            final code = controller.text.trim();
            if (code.isNotEmpty) onSubmit(code);
          },
        ),
      ],
    );
  }
}

/// The phase the linking flow is in.
enum _LinkPhase { connecting, awaitingConfirmation, linking, failed }

/// Fourth page: drives the linking process once it has started.
class _LinkingPage extends HookWidget {
  const _LinkingPage({
    required this.sessionId,
    required this.onBack,
    required this.startLinkSession,
  });

  final String sessionId;
  final VoidCallback onBack;
  final LinkSessionStarter startLinkSession;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    // Starts linking once; the same session (event stream + confirm callback) is
    // reused across rebuilds.
    final session = useMemoized(() => startLinkSession(context, sessionId));

    final phase = useState(_LinkPhase.connecting);

    // Subscribe to the linking event stream while this page is mounted.
    useEffect(() {
      final subscription = session.events.listen((event) {
        switch (event) {
          case MultiDeviceLinkEvent_AwaitingConfirmation():
            // Connected: prompt the user (unless they already confirmed).
            if (phase.value == _LinkPhase.connecting) {
              phase.value = _LinkPhase.awaitingConfirmation;
            }
          case MultiDeviceLinkEvent_Linked():
            // TODO: this should end the entire process later
            if (context.mounted) _showLinkedDialog(context);
          case MultiDeviceLinkEvent_Failed():
            phase.value = _LinkPhase.failed;
        }
      }, onError: (Object _) => phase.value = _LinkPhase.failed);
      return subscription.cancel;
    }, const []);

    return switch (phase.value) {
      _LinkPhase.connecting => _LinkStatusView(
        title: loc.linkedDevicesScreen_linkDevice,
        message: loc.linkingDeviceScreen_connecting,
      ),
      _LinkPhase.awaitingConfirmation => _LinkConfirmView(
        onBack: onBack,
        onConfirm: () {
          session.confirm();
          phase.value = _LinkPhase.linking;
        },
      ),
      _LinkPhase.linking => _LinkStatusView(
        title: loc.linkedDevicesScreen_linkDevice,
        message: loc.linkingDeviceScreen_linking,
      ),
      _LinkPhase.failed => _LinkErrorView(onBack: onBack),
    };
  }

  /// Closes the link modal and shows a success popup.
  void _showLinkedDialog(BuildContext context) {
    if (!context.mounted) return;
    final navigator = Navigator.of(context);
    navigator.pop();
    showDialog<void>(
      context: navigator.context,
      builder: (_) => const ConfirmDialog(
        title: "Device was linked! 🎉",
        message: "Your new device is now linked to your account.",
        confirm: "OK",
      ),
    );
  }
}

/// A modal page showing a centered spinner with a status [message] under the
/// [title] header (used for the connecting and linking phases).
class _LinkStatusView extends StatelessWidget {
  const _LinkStatusView({required this.title, required this.message});

  final String title;
  final String message;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _LinkModalHeader(title: title),
        const SizedBox(height: Spacing.px24),
        Text(
          message,
          textAlign: TextAlign.center,
          style: TextStyle(
            color: colors.text.secondary,
            fontSize: LabelFontSize.small1.size,
          ),
        ),
        const SizedBox(height: Spacing.px24),
        const Center(child: CircularProgressIndicator()),
      ],
    );
  }
}

/// The failure page for the acceptor-side linking flow.
class _LinkErrorView extends StatelessWidget {
  const _LinkErrorView({required this.onBack});

  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _LinkModalHeader(
          title: loc.linkingDevicesScreen_error_title,
          onBack: onBack,
        ),
        const SizedBox(height: Spacing.px24),
        Text(
          loc.linkingDevicesScreen_error_generic,
          textAlign: TextAlign.center,
          style: TextStyle(
            color: colors.text.primary,
            fontSize: BodyFontSize.small2.size,
          ),
        ),
        const SizedBox(height: Spacing.px24),
        AppButton(
          label: loc.linkingDevicesScreen_error_dismiss,
          onPressed: onBack,
        ),
      ],
    );
  }
}

/// Text field with hint for naming the device being linked.
class _LinkDeviceName extends StatelessWidget {
  const _LinkDeviceName({required this.textEditingController});

  final TextEditingController textEditingController;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return Column(
      spacing: Spacing.px8,
      crossAxisAlignment: .start,
      children: [
        Container(
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(Spacing.px16),
            color: colors.backgroundBase.secondary,
          ),
          padding: const EdgeInsets.only(
            left: Spacing.px12,
            right: Spacing.px12,
          ),
          child: Row(
            mainAxisAlignment: .start,
            crossAxisAlignment: .center,
            spacing: Spacing.px8,
            children: [
              const AppIcon.laptop(),
              Expanded(
                child: TextField(
                  controller: textEditingController,
                  maxLength: 30,
                  buildCounter:
                      (
                        _, {
                        required currentLength,
                        required isFocused,
                        maxLength,
                      }) => null,
                ),
              ),
            ],
          ),
        ),
        Text(
          loc.linkingDeviceScreen_linking_confirm_edit_subtitle,
          style: TextStyle(
            color: colors.text.tertiary,
            fontSize: LabelFontSize.small2.size,
          ),
        ),
      ],
    );
  }
}

/// Confirmation gate shown once the relay connection is up: the user must tick
/// the checkbox before the link is completed.
class _LinkConfirmView extends HookWidget {
  const _LinkConfirmView({required this.onBack, required this.onConfirm});

  final VoidCallback onBack;
  final VoidCallback onConfirm;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    final checked = useState(false);

    final platform = Theme.of(context).platform;
    final deviceName = useTextEditingController(
      text: initialDeviceName(platform),
    );

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _LinkModalHeader(
          title: loc.linkingDeviceScreen_linking_confirm_title,
          onBack: onBack,
        ),
        const SizedBox(height: Spacing.px24),
        _LinkDeviceName(textEditingController: deviceName),
        const SizedBox(height: Spacing.px16),
        Text(
          loc.linkingDeviceScreen_linking_confirm_body,
          style: TextStyle(
            color: colors.function.danger,
            fontSize: LabelFontSize.small1.size,
          ),
        ),
        const SizedBox(height: Spacing.px16),
        InkWell(
          onTap: () => checked.value = !checked.value,
          borderRadius: BorderRadius.circular(Spacing.px8),
          child: Row(
            children: [
              Checkbox(
                value: checked.value,
                onChanged: (value) => checked.value = value ?? false,
              ),
              Expanded(
                child: Text(
                  loc.linkingDeviceScreen_linking_confirm_checkbox,
                  style: TextStyle(
                    color: colors.text.primary,
                    fontSize: LabelFontSize.small1.size,
                  ),
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: Spacing.px24),
        AppButton(
          type: .primary,
          label: loc.linkingDeviceScreen_linking_confirm_button,
          state: checked.value
              ? AppButtonState.active
              : AppButtonState.inactive,
          onPressed: onConfirm,
        ),
      ],
    );
  }
}
