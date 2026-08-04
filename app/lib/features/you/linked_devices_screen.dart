// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/ds/patterns/edit_dialog/edit_dialog.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/icon_badge/app_icon_badge.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/you/linking_device_dialog.dart';
import 'package:air/features/you/you_fields.dart';
import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

/// The devices section: this device, the ones linked to it, and the way to add
/// another. Sized and scrolled by its host.
class LinkedDevicesContent extends StatelessWidget {
  const LinkedDevicesContent({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final platform = Theme.of(context).platform;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          loc.linkedDevicesScreen_thisDevice,
          style: typeScale.body.regular.style(
            weight: Weight.emphasized,
            color: palette.text.secondary,
          ),
        ),
        const SizedBox(height: S.s8),
        _SingleDevice(
          deviceName: platform.name,
          linkedAt: DateTime.parse("2026-01-15 02:45:00"),
        ),
        const SizedBox(height: S.s24),
        Text(
          loc.linkedDevicesScreen_linkedDevices,
          style: typeScale.body.regular.style(
            weight: Weight.emphasized,
            color: palette.text.secondary,
          ),
        ),
        const SizedBox(height: S.s8),
        _SingleDevice(
          deviceName: "iOS",
          linkedAt: DateTime.parse("2026-02-03 14:22:00"),
          unlinkIcon: true,
        ),
        const SizedBox(height: S.s8),
        _SingleDevice(
          deviceName: "Android",
          linkedAt: DateTime.parse("2026-03-20 10:12:00"),
          unlinkIcon: true,
        ),
        const SizedBox(height: S.s8),
        Text(
          loc.linkedDevicesScreen_editNameHint,
          style: typeScale.body.xs.style(color: palette.text.quaternary),
        ),
        const SizedBox(height: S.s24),
        Button(
          type: .primary,
          label: loc.linkedDevicesScreen_linkDevice,
          onPressed: () => showDialog(
            context: context,
            builder: (_) => const LinkDeviceModal(),
          ),
        ),
        const SizedBox(height: S.s8),
        SizedBox(
          width: .infinity,
          child: Text(
            loc.linkedDevicesScreen_deviceCount(5, 5),
            textAlign: .center,
            style: typeScale.body.xs.style(color: palette.text.quaternary),
          ),
        ),
        const SizedBox(height: S.s4),
        const SizedBox(width: .infinity, child: _EncryptionNotice()),
      ],
    );
  }
}

/// The end-to-end encryption footer.
class _EncryptionNotice extends StatelessWidget {
  const _EncryptionNotice();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final textStyle = typeScale.body.xs.style(color: palette.text.quaternary);

    return Column(
      mainAxisAlignment: .center,
      spacing: S.s8,
      children: [
        Text(loc.linkedDevicesScreen_encryptionNotice, style: textStyle),
        GestureDetector(
          child: Text(
            loc.linkedDevicesScreen_encryptionNotice_learnMore,
            style: textStyle.copyWith(color: palette.function.link),
          ),
          onTap: () => showAdaptiveConfirm(
            context: context,
            title: loc.linkedDevicesScreen_encryptionDialog_title,
            description: loc.linkedDevicesScreen_encryptionDialog_content,
            primaryActionText: loc.linkedDevicesScreen_encryptionDialog_confirm,
          ),
        ),
      ],
    );
  }
}

/// A tappable entry for a single linked device in the "Devices" view.
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
    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);
    final locale = Localizations.localeOf(context).toString();
    final dateFormat = DateFormat.yMMMMd(locale).addPattern("'at'").add_jm();

    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(CornerRadius.px16),
        color: youModuleFill(context),
      ),
      padding: const EdgeInsets.all(S.s12),
      child: Row(
        spacing: S.s16,
        children: [
          AppIconBadge(
            type: .laptop,
            size: 24,
            backgroundColor: palette.backgroundBase.quaternary,
          ),
          Expanded(
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => _editDeviceName(context),
              child: Column(
                spacing: S.s4,
                mainAxisAlignment: .start,
                crossAxisAlignment: .start,
                children: [
                  Text(
                    deviceName,
                    style: typeScale.body.regular.style(
                      color: palette.text.primary,
                    ),
                  ),
                  Text(
                    loc.linkedDevicesScreen_linkedOn(
                      dateFormat.format(linkedAt),
                    ),
                    style: typeScale.body.xs.style(
                      color: palette.text.tertiary,
                    ),
                  ),
                ],
              ),
            ),
          ),
          GestureDetector(
            onTap: () => _unlinkDevice(context),
            child: AppIcon.trash(color: palette.function.danger, size: 24),
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
