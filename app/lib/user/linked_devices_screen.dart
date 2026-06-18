// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/app_scaffold.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/modal/confirm_dialog.dart';
import 'package:air/ds/components/modal/edit_dialog.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/spacing.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/theme/styles.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/user/linking_device_dialog.dart';
import 'package:air/user/user_settings_cubit.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:intl/intl.dart';

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
    final platform = Theme.of(context).platform;

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
                  deviceName: platform.name,
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
                    builder: (_) => const LinkDeviceModal(),
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
