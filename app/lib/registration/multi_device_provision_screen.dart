// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/modal/confirm_dialog.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/components/desktop/width_constraints.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/registration/registration_cubit.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_svg/svg.dart';

/// The phase a linking session is in.
sealed class _LinkingPhase {
  const _LinkingPhase();
}

/// Connecting to the relay; no code assigned yet.
class _Connecting extends _LinkingPhase {
  const _Connecting();
}

/// The relay assigned a [code]; waiting for the existing device to link.
class _AwaitingLink extends _LinkingPhase {
  const _AwaitingLink({required this.code, required this.qrcodeSvg});
  final String code;
  final String? qrcodeSvg;
}

/// The existing device has established the session.
class _Linking extends _LinkingPhase {
  _Linking();
}

/// The existing device connected and linking completed.
class _Linked extends _LinkingPhase {
  _Linked({required this.answer});
  final String answer;
}

/// The session failed or expired.
class _Failed extends _LinkingPhase {
  const _Failed({required this.message});
  final String message;
}

/// Source of provisioning events for [MultiDeviceProvisionScreen].
/// Defaults to the real Rust-bridge [multiDeviceProvisionClient] but made
/// injectable so tests can drive the screen through each phase.
typedef MultiDeviceProvisionClientFactory =
    Stream<MultiDeviceProvisionEvent> Function({required String domain});

class MultiDeviceProvisionScreen extends HookWidget {
  const MultiDeviceProvisionScreen({
    super.key,
    this.provisionClient = multiDeviceProvisionClient,
  });

  /// The provisioning event source. Overridden in tests.
  final MultiDeviceProvisionClientFactory provisionClient;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    // Can be set by the hidden input field
    final domain = context.select(
      (RegistrationCubit cubit) => cubit.state.domain,
    );

    // Bumping this restarts the linking session (cancels the old stream).
    final attempt = useState(0);
    final phase = useState<_LinkingPhase>(const _Connecting());

    // Subscribe to the provisioning stream while the screen is mounted. The
    // subscription keeps the session alive, and cancelling it
    // (on dispose or reload) tears it down.
    useEffect(() {
      phase.value = const _Connecting();
      final subscription = provisionClient(domain: domain).listen(
        (event) {
          switch (event) {
            case MultiDeviceProvisionEvent_Code(:final code, :final qrcodeSvg):
              phase.value = _AwaitingLink(code: code, qrcodeSvg: qrcodeSvg);
            case MultiDeviceProvisionEvent_Linking():
              phase.value = _Linking();
            case MultiDeviceProvisionEvent_Linked(:final field0):
              phase.value = _Linked(answer: field0);
            case MultiDeviceProvisionEvent_Failed(:final field0):
              phase.value = _Failed(message: field0);
          }
        },
        onError: (Object error) {
          phase.value = _Failed(
            message: loc.linkingDevicesScreen_error_generic,
          );
        },
        onDone: () {
          // the stream has been closed from the Rust side (i.e. timeout)
          if (phase.value is! _Linked) {
            phase.value = _Failed(
              message: loc.linkingDeviceScreen_error_codesExpired_message,
            );
          }
        },
      );
      return subscription.cancel;
    }, [domain, attempt.value]);

    void reload() => attempt.value++;

    // When the session fails (timed out, connection dropped, or a stream error),
    // surface it as a modal over the screen, showing the failure message.
    //
    // Reload restarts the session and cancel navigates back.
    final failure = phase.value;
    final failureMessage = failure is _Failed ? failure.message : null;
    useEffect(() {
      if (failureMessage == null) {
        return null;
      }
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _showLinkingFailedDialog(
          context,
          message: failureMessage,
          onReload: reload,
        );
      });
      return null;
    }, [failureMessage]);

    return Scaffold(
      resizeToAvoidBottomInset: true,
      appBar: AppBar(
        clipBehavior: Clip.none,
        leading: AppBarBackButton(
          backgroundColor: colors.backgroundElevated.primary,
        ),
        title: Text(
          loc.linkingDeviceScreen_header,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        toolbarHeight: isPointer() ? 100 : null,
        backgroundColor: colors.backgroundBase.secondary,
      ),
      backgroundColor: colors.backgroundBase.secondary,
      body: SafeArea(
        child: Center(
          child: ConstrainedWidth(
            width: 500,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: Spacing.px16),
              child: switch (phase.value) {
                _Connecting() => const _ConnectingView(),
                _AwaitingLink(:final code, :final qrcodeSvg) =>
                  _AwaitingLinkView(code: code, qrcodeSvg: qrcodeSvg),
                _Linking() => const _LinkingView(),
                _Linked() => const _LinkedView(),
                // The failure is shown as a modal
                _Failed() => const SizedBox.expand(),
              },
            ),
          ),
        ),
      ),
    );
  }
}

class _NumberedBullet extends StatelessWidget {
  const _NumberedBullet(this.index);

  final int index;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Text(
      style: TextStyle(
        fontSize: BodyFontSize.base.size,
        color: colors.text.primary,
        fontWeight: FontWeight.bold,
      ),
      "$index.",
    );
  }
}

class _LinkingInstructionsList extends StatelessWidget {
  const _LinkingInstructionsList({required this.lines});

  final List<String> lines;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Column(
      spacing: Spacing.px4,
      mainAxisAlignment: .center,
      children: [
        for (final (idx, line) in lines.indexed)
          Row(
            spacing: Spacing.px8,
            mainAxisAlignment: .start,
            crossAxisAlignment: .baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              _NumberedBullet(idx + 1),
              Flexible(
                child: Text(
                  style: TextStyle(
                    fontSize: LabelFontSize.base.size,
                    color: colors.text.primary,
                  ),
                  line,
                ),
              ),
            ],
          ),
      ],
    );
  }
}

/// When connecting to the relay server
class _ConnectingView extends StatelessWidget {
  const _ConnectingView();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return Column(
      mainAxisAlignment: .center,
      spacing: Spacing.px16,
      children: [
        Text(
          loc.linkingDeviceScreen_connecting,
          style: TextStyle(
            fontSize: LabelFontSize.base.size,
            color: colors.text.secondary,
          ),
        ),
        const CircularProgressIndicator(),
      ],
    );
  }
}

/// Small widget to show a remaining duration inside of
/// a CircularProgressIndicator
class _CountdownRing extends HookWidget {
  const _CountdownRing({
    required this.duration,
    required this.warnThreshold,
    required this.size,
  });

  final Duration duration;
  final Duration warnThreshold;
  final double size;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    final controller = useAnimationController(duration: duration);

    useEffect(() {
      // Run the ring from full (1.0) down to empty (0.0)
      controller.reverse(from: 1.0);
      return null;
    }, [controller]);

    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) {
        final remaining = (controller.value * duration.inSeconds).ceil();
        final ringColor = remaining <= warnThreshold.inSeconds
            ? colors.function.warning
            : colors.function.success;

        return SizedBox(
          width: size,
          height: size,
          child: Stack(
            alignment: Alignment.center,
            children: [
              // Mirror horizontally so the ring depletes counterclockwise.
              Transform.scale(
                scaleX: -1,
                child: SizedBox.expand(
                  child: Padding(
                    padding: const EdgeInsets.all(1),
                    child: CircularProgressIndicator(
                      value: controller.value,
                      strokeWidth: 2,
                      color: ringColor,
                      backgroundColor: colors.backgroundBase.tertiary,
                    ),
                  ),
                ),
              ),
              Text(
                "$remaining",
                style: TextStyle(
                  fontSize: LabelFontSize.base.size,
                  fontWeight: FontWeight.bold,
                  fontFeatures: const [FontFeature.tabularFigures()],
                  color: colors.text.primary,
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// Map colors of the QR code generated on the Rust side to the custom
/// colors of the design system we use in Flutter.
class _LinkQrCodeSvgColorMapper extends ColorMapper {
  const _LinkQrCodeSvgColorMapper({required this.colors});

  final CustomColorScheme colors;

  @override
  Color substitute(
    String? id,
    String elementName,
    String attributeName,
    Color color,
  ) {
    return switch (color) {
      Colors.black => colors.function.toggleBlack,
      Colors.white => colors.backgroundBase.secondary,
      _ => color,
    };
  }
}

/// Main view of this screen, with the QR code and waiting indicator
class _AwaitingLinkView extends StatelessWidget {
  const _AwaitingLinkView({required this.code, required this.qrcodeSvg});

  final String code;
  final String? qrcodeSvg;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final svg = qrcodeSvg;

    return Column(
      children: [
        Container(
          width: double.infinity,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(Spacing.px16),
            border: Border.all(
              color: colors.backgroundBase.quaternary,
              width: 1,
            ),
          ),
          padding: const EdgeInsets.all(Spacing.px16),
          child: Column(
            mainAxisAlignment: .center,
            crossAxisAlignment: .center,
            mainAxisSize: .min,
            children: [
              if (svg != null)
                SvgPicture.string(
                  svg,
                  width: 200, // Easily override or set width
                  height: 200, // Easily override or set height
                  colorMapper: _LinkQrCodeSvgColorMapper(colors: colors),
                  placeholderBuilder: (BuildContext context) =>
                      const CircularProgressIndicator(),
                ),
              const SizedBox(height: Spacing.px16),
              Text(
                loc.linkingDeviceScreen_separator,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: LabelFontSize.base.size,
                  color: colors.text.tertiary,
                ),
              ),
              const SizedBox(height: Spacing.px8),
              Text(
                loc.linkingDeviceScreen_numericCode,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: LabelFontSize.small1.size,
                  color: colors.text.primary,
                ),
              ),
              const SizedBox(height: Spacing.px8),
              Text(
                code,
                textAlign: TextAlign.center,
                style: TextStyle(
                  fontSize: HeaderFontSize.h1.size,
                  fontWeight: FontWeight.bold,
                  fontFeatures: const [FontFeature.tabularFigures()],
                  letterSpacing: 4,
                  color: colors.text.primary,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: Spacing.px24),
        const _CountdownRing(
          duration: Duration(seconds: 59),
          warnThreshold: Duration(seconds: 10),
          size: 48,
        ),
        const SizedBox(height: Spacing.px32),
        _LinkingInstructionsList(
          lines: [
            loc.linkingDeviceScreen_instructions_1,
            loc.linkingDeviceScreen_instructions_2,
            loc.linkingDeviceScreen_instructions_3,
          ],
        ),
      ],
    );
  }
}

/// When linking with an existing client
class _LinkingView extends StatelessWidget {
  const _LinkingView();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return Column(
      mainAxisAlignment: .center,
      spacing: Spacing.px16,
      children: [
        Text(
          loc.linkingDeviceScreen_linking,
          style: TextStyle(
            fontSize: LabelFontSize.base.size,
            color: colors.text.secondary,
          ),
        ),
        const CircularProgressIndicator(),
      ],
    );
  }
}

// TODO: this should not exist and will be a loading chat list instead?
class _LinkedView extends StatelessWidget {
  const _LinkedView();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return Column(
      mainAxisAlignment: .center,
      spacing: Spacing.px16,
      children: [
        Icon(Icons.check_circle, size: 48, color: colors.accent.primary),
        Text(
          loc.linkingDeviceScreen_linked,
          style: TextStyle(
            fontSize: BodyFontSize.base.size,
            fontWeight: FontWeight.bold,
            color: colors.text.primary,
          ),
        ),
      ],
    );
  }
}

/// Shows the failure modal over the screen and acts on the user's choice:
/// reload (restart the session) via [onReload], or leave the linking flow.
Future<void> _showLinkingFailedDialog(
  BuildContext context, {
  required String message,
  required VoidCallback onReload,
}) async {
  if (!context.mounted) {
    return;
  }
  final loc = AppLocalizations.of(context);
  final shouldReload = await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (_) => ConfirmDialog(
      message: message,
      title: loc.linkingDeviceScreen_error_codesExpired_title,
      cancel: loc.linkingDeviceScreen_error_codesExpired_cancel,
      confirm: loc.linkingDeviceScreen_error_codesExpired_reload,
    ),
  );
  if (!context.mounted) {
    return;
  }
  if (shouldReload ?? false) {
    onReload();
  } else {
    context.read<NavigationCubit>().pop();
  }
}
