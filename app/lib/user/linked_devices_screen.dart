// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/app_scaffold.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/components/modal/app_dialog.dart';
import 'package:air/ds/components/modal/confirm_dialog.dart';
import 'package:air/ds/components/modal/edit_dialog.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/theme/styles.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/user_cubit.dart';
import 'package:air/user/user_settings_cubit.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:intl/intl.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

class LinkedDevicesScreen extends StatelessWidget {
  const LinkedDevicesScreen({super.key, required this.userSettingsCubit});

  final UserSettingsCubit userSettingsCubit;

  @override
  Widget build(BuildContext context) {
    return BlocProvider<UserSettingsCubit>.value(
      value: userSettingsCubit,
      child: const LinkedDevicesView(),
    );
  }
}

class LinkedDevicesView extends HookWidget {
  const LinkedDevicesView({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return AppScaffold(
      title: loc.linkedDevicesScreen_title,
      backgroundColor: colors.backgroundBase.primary,
      child: Align(
        alignment: Alignment.topCenter,
        child: Container(
          constraints: isPointer() ? const BoxConstraints(maxWidth: 800) : null,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  loc.linkedDevicesScreen_thisDevice,
                  style: TextStyle(
                    fontSize: LabelFontSize.base.size,
                    fontWeight: FontWeight.bold,
                    color: colors.text.secondary,
                  ),
                ),
                const SizedBox(height: Spacing.px8),
                _SingleDevice(
                  deviceName: "Linux",
                  linkedAt: DateTime.parse("2026-01-15 02:45:00"),
                ),
                const SizedBox(height: Spacing.px24),
                Text(
                  loc.linkedDevicesScreen_linkedDevices,
                  style: TextStyle(
                    fontSize: LabelFontSize.base.size,
                    fontWeight: FontWeight.bold,
                    color: colors.text.secondary,
                  ),
                ),
                const SizedBox(height: Spacing.px8),
                _SingleDevice(
                  deviceName: "iOS",
                  linkedAt: DateTime.parse("2026-02-03 14:22:00"),
                  unlinkIcon: true,
                ),
                const SizedBox(height: Spacing.px8),
                _SingleDevice(
                  deviceName: "Android",
                  linkedAt: DateTime.parse("2026-03-20 10:12:00"),
                  unlinkIcon: true,
                ),
                const SizedBox(height: Spacing.px8),
                Text(
                  loc.linkedDevicesScreen_editNameHint,
                  style: TextStyle(
                    fontSize: LabelFontSize.small2.size,
                    color: colors.text.quaternary,
                  ),
                ),
                const SizedBox(height: Spacing.px24),
                AppButton(
                  type: .primary,
                  label: loc.linkedDevicesScreen_linkDevice,
                  onPressed: () => showDialog(
                    context: context,
                    builder: (_) => const _LinkDeviceModal(),
                  ),
                ),
                const SizedBox(height: Spacing.px8),
                SizedBox(
                  width: .infinity,
                  child: Text(
                    loc.linkedDevicesScreen_deviceCount(5, 5),
                    textAlign: .center,
                    style: TextStyle(
                      fontSize: LabelFontSize.small2.size,
                      color: colors.text.quaternary,
                    ),
                  ),
                ),
                const SizedBox(height: Spacing.px4),
                const SizedBox(width: .infinity, child: _EncryptionNotice()),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The end-to-end encryption footer, with an inline "Learn more." link that
/// opens [_showEncryptionInfoDialog].
class _EncryptionNotice extends StatelessWidget {
  const _EncryptionNotice();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final baseStyle = TextStyle(
      fontSize: LabelFontSize.small2.size,
      color: colors.text.quaternary,
    );

    final linkText = loc.linkedDevicesScreen_encryptionNotice_learnMore;
    final notice = loc.linkedDevicesScreen_encryptionNotice(linkText);
    final linkStart = notice.indexOf(linkText);

    if (linkStart == -1) {
      return Text(notice, style: baseStyle, textAlign: TextAlign.center);
    }

    return Text.rich(
      TextSpan(
        style: baseStyle,
        children: [
          TextSpan(text: notice.substring(0, linkStart)),
          TextSpan(
            text: linkText,
            style: baseStyle.copyWith(color: colors.function.link),
            recognizer: TapGestureRecognizer()
              ..onTap = () => showDialog(
                context: context,
                builder: (_) => ConfirmDialog(
                  title: loc.linkedDevicesScreen_encryptionDialog_title,
                  message: loc.linkedDevicesScreen_encryptionDialog_content,
                  confirm: loc.linkedDevicesScreen_encryptionDialog_confirm,
                ),
              ),
          ),
          TextSpan(text: notice.substring(linkStart + linkText.length)),
        ],
      ),
      textAlign: TextAlign.center,
    );
  }
}

/// The page currently shown inside [_LinkDeviceModal].
enum _LinkPage { chooser, scanQrCode, numericCode, linking }

/// Entry point for linking a new device. A small multi-page modal: the first
/// page explains the flow and lets the user pick a method; choosing a method
/// swaps the content to the matching page in place (with a back arrow).
class _LinkDeviceModal extends HookWidget {
  const _LinkDeviceModal();

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
        ),
      },
    );
  }
}

/// Modal header for the link-device pages: a centered, bold [title] with an
/// optional back arrow at the top-left, mirroring [AppScaffold]'s app bar.
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

/// First page: explains the flow and offers the two linking methods.
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
          label: loc.linkedDevicesScreen_linkDialog_scanQrCode,
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
///
/// Uses the device camera via [MobileScanner] on mobile. On platforms without
/// camera scanning support (desktop/web) it falls back to a placeholder; those
/// users are steered towards the numeric-code flow anyway.
///
/// consumes a linking code exists.
class _ScanQrCodePage extends StatefulWidget {
  const _ScanQrCodePage({required this.onBack, required this.onCodeScanned});

  final VoidCallback onBack;

  /// Called with the linking code once a linking QR is scanned.
  final ValueChanged<String> onCodeScanned;

  /// Whether live camera scanning is available on this platform.
  static bool get _scannerSupported =>
      !kIsWeb &&
      (defaultTargetPlatform == TargetPlatform.android ||
          defaultTargetPlatform == TargetPlatform.iOS);

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
      // Parsing lives in Rust (the cubit), the single source of truth for the
      // URL format, and validates the code targets our own home server.
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
              child: _ScanQrCodePage._scannerSupported
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

/// Fourth page: drives the acceptor side of linking for a scanned or entered
/// code, showing progress until it completes (then closes the modal) or fails.
class _LinkingPage extends StatefulWidget {
  const _LinkingPage({required this.sessionId, required this.onBack});

  final String sessionId;
  final VoidCallback onBack;

  @override
  State<_LinkingPage> createState() => _LinkingPageState();
}

class _LinkingPageState extends State<_LinkingPage> {
  late final Future<String> _linking = context.read<UserCubit>().linkDevice(
    widget.sessionId,
  );

  /// Latched once the success popup has been shown, so repeated rebuilds of the
  /// completed [FutureBuilder] don't stack multiple dialogs.
  bool _shownSuccess = false;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return FutureBuilder<String>(
      future: _linking,
      builder: (context, snapshot) {
        if (snapshot.connectionState == ConnectionState.done &&
            !snapshot.hasError) {
          // Linking succeeded: swap the modal for a celebratory popup once this
          // frame settles.
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (!context.mounted || _shownSuccess) return;
            _shownSuccess = true;
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
          });
        }

        if (snapshot.hasError) {
          debugPrint("$snapshot.error!");
        }

        final loc = AppLocalizations.of(context);

        return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _LinkModalHeader(
              title: loc.linkedDevicesScreen_linkDevice,
              onBack: snapshot.hasError ? widget.onBack : null,
            ),
            const SizedBox(height: Spacing.px24),
            if (snapshot.hasError)
              Text(
                loc.linkingDeviceScreen_error_generic,
                textAlign: TextAlign.center,
                style: TextStyle(
                  color: colors.function.danger,
                  fontSize: BodyFontSize.small2.size,
                ),
              )
            else if (snapshot.hasData)
              Center(child: Text(snapshot.data!))
            else
              const Center(child: CircularProgressIndicator()),
          ],
        );
      },
    );
  }
}

/// An [AppIcon] wrapped in a rounded/circular background.
class _AppIconBadge extends StatelessWidget {
  const _AppIconBadge({
    required this.size,
    required this.type,
    this.backgroundColor,
  });

  final AppIconType type;
  final double size;
  final Color? backgroundColor;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Container(
      padding: EdgeInsets.all(size / 2),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: backgroundColor ?? colors.backgroundBase.tertiary,
        shape: BoxShape.rectangle,
        borderRadius: BorderRadius.circular(Spacing.px12),
      ),
      child: AppIcon(type: type, size: size),
    );
  }
}

class _SingleDevice extends StatelessWidget {
  const _SingleDevice({
    required this.deviceName,
    required this.linkedAt,
    this.unlinkIcon = false,
  });

  final String deviceName;
  final DateTime linkedAt;
  final bool unlinkIcon;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);
    final locale = Localizations.localeOf(context).toString();
    final dateFormat = DateFormat.yMMMMd(locale).addPattern("'at'").add_jm();

    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(Spacing.px16),
        color: colors.backgroundBase.secondary,
      ),
      padding: const EdgeInsets.all(Spacing.px12),
      child: Row(
        spacing: Spacing.px16,
        children: [
          _AppIconBadge(
            type: .laptop,
            size: 24,
            backgroundColor: colors.backgroundBase.quaternary,
          ),
          Expanded(
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => _editDeviceName(context),
              child: Column(
                spacing: Spacing.px4,
                mainAxisAlignment: .start,
                crossAxisAlignment: .start,
                children: [
                  Text(
                    deviceName,
                    style: TextStyle(
                      fontSize: LabelFontSize.base.size,
                      color: colors.text.primary,
                    ),
                  ),
                  Text(
                    loc.linkedDevicesScreen_linkedOn(
                      dateFormat.format(linkedAt),
                    ),
                    style: TextStyle(
                      fontSize: LabelFontSize.small2.size,
                      color: colors.text.tertiary,
                    ),
                  ),
                ],
              ),
            ),
          ),
          GestureDetector(
            onTap: () => _unlinkDevice(context),
            child: AppIcon.trash(color: colors.function.danger, size: 24),
          ),
        ],
      ),
    );
  }

  void _editDeviceName(BuildContext context) {
    final loc = AppLocalizations.of(context);
    showDialog(
      context: context,
      builder: (_) => EditDialog(
        title: loc.linkedDevicesScreen_deviceName_editDialog_title,
        cancel: loc.linkedDevicesScreen_deviceName_editDialog_cancel,
        confirm: loc.linkedDevicesScreen_deviceName_editDialog_confirm,
        initialValue: deviceName,
        maxLength: 30,
        validator: (value) => value.trim().isNotEmpty,
        // NOOP for now
        onSubmit: (_) => Navigator.of(context).pop(),
      ),
    );
  }

  void _unlinkDevice(BuildContext context) {
    final loc = AppLocalizations.of(context);
    showDialog(
      context: context,
      builder: (_) => ConfirmDialog(
        title: loc.linkedDevicesScreen_unlinkDialog_title,
        message: loc.linkedDevicesScreen_unlinkDialog_content,
        cancel: loc.linkedDevicesScreen_unlinkDialog_cancel,
        confirm: loc.linkedDevicesScreen_unlinkDialog_confirm,
        destructive: true,
        onConfirm: () {
          // NOOP for now
        },
      ),
    );
  }
}
