// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/icon_badge/app_icon_badge.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/ds/patterns/edit_dialog/edit_dialog.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/you/linked_devices_cubit.dart';
import 'package:air/features/you/linking_device_dialog.dart';
import 'package:air/features/you/you_fields.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:intl/intl.dart';

/// Wire platform codes from `protos/src/client/self_group.rs`.
const _platformAndroid = 1;
const _platformIos = 2;

AppIconType _iconFor(int platform) => switch (platform) {
  _platformAndroid || _platformIos => AppIconType.smartphone,
  // macOS, Windows, Linux and unknown platforms.
  _ => AppIconType.laptop,
};

/// The devices section: this device, the ones linked to it, and the way to add
/// another. Sized and scrolled by its host.
class LinkedDevicesContent extends StatelessWidget {
  const LinkedDevicesContent({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) =>
          LinkedDevicesCubit(userCubit: context.read<UserCubit>()),
      child: const LinkedDevicesView(),
    );
  }
}

class LinkedDevicesView extends StatelessWidget {
  const LinkedDevicesView({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    final devices = context.select(
      (LinkedDevicesCubit cubit) => cubit.state.devices,
    );
    final thisDevice = devices
        .where((device) => device.isThisDevice)
        .firstOrNull;
    final others = devices.where((device) => !device.isThisDevice).toList();

    final sectionStyle = typeScale.body.regular.style(
      weight: Weight.emphasized,
      color: palette.text.secondary,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (thisDevice != null) ...[
          Text(loc.linkedDevicesScreen_thisDevice, style: sectionStyle),
          const SizedBox(height: S.s8),
          _SingleDevice(device: thisDevice),
          const SizedBox(height: S.s24),
        ],
        if (others.isNotEmpty) ...[
          Text(loc.linkedDevicesScreen_linkedDevices, style: sectionStyle),
          const SizedBox(height: S.s8),
          for (final device in others) ...[
            _SingleDevice(device: device),
            const SizedBox(height: S.s8),
          ],
          Text(
            loc.linkedDevicesScreen_editNameHint,
            style: typeScale.body.xs.style(color: palette.text.quaternary),
          ),
          const SizedBox(height: S.s24),
        ],
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
            loc.linkedDevicesScreen_deviceCount(others.length),
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
        MouseRegion(
          cursor: SystemMouseCursors.click,
          child: GestureDetector(
            child: Text(
              loc.linkedDevicesScreen_encryptionNotice_learnMore,
              style: textStyle.copyWith(color: palette.function.link),
            ),
            onTap: () => showAdaptiveConfirm(
              context: context,
              title: loc.linkedDevicesScreen_encryptionDialog_title,
              description: loc.linkedDevicesScreen_encryptionDialog_content,
              primaryActionText:
                  loc.linkedDevicesScreen_encryptionDialog_confirm,
            ),
          ),
        ),
      ],
    );
  }
}

/// A tappable entry for a single linked device in the "Devices" view.
class _SingleDevice extends StatelessWidget {
  const _SingleDevice({required this.device});

  final UiLinkedDevice device;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);
    final locale = Localizations.localeOf(context).toString();
    final dateFormat = DateFormat.yMMMMd(locale).addPattern("'at'").add_jm();
    final name = device.name.isEmpty
        ? loc.linkedDevicesScreen_unknownDevice
        : device.name;
    final linkedAt = device.linkedAt;

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
            type: _iconFor(device.platform),
            size: 24,
            backgroundColor: palette.backgroundBase.quaternary,
          ),
          Expanded(
            child: MouseRegion(
              cursor: SystemMouseCursors.click,
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: () => _editDeviceName(context, name),
                child: Column(
                  spacing: S.s4,
                  mainAxisAlignment: .start,
                  crossAxisAlignment: .start,
                  children: [
                    Text(
                      name,
                      style: typeScale.body.regular.style(
                        color: palette.text.primary,
                      ),
                    ),
                    if (linkedAt != null)
                      Text(
                        loc.linkedDevicesScreen_linkedOn(
                          dateFormat.format(linkedAt.toLocal()),
                        ),
                        style: typeScale.body.xs.style(
                          color: palette.text.tertiary,
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ),
          // The own device is never unlinkable from here: a device is unlinked
          // from one of its siblings.
          if (!device.isThisDevice)
            GestureDetector(
              onTap: () => _unlinkDevice(context, name),
              child: AppIcon.trash(color: palette.function.danger, size: 24),
            ),
        ],
      ),
    );
  }

  void _unlinkDevice(BuildContext context, String name) {
    final loc = AppLocalizations.of(context);
    final cubit = context.read<LinkedDevicesCubit>();
    showDialog(
      context: context,
      builder: (_) => ConfirmDialog(
        title: loc.linkedDevicesScreen_unlinkDialog_title,
        message: loc.linkedDevicesScreen_unlinkDialog_content,
        cancel: loc.linkedDevicesScreen_unlinkDialog_cancel,
        confirm: loc.linkedDevicesScreen_unlinkDialog_confirm,
        destructive: true,
        onConfirm: () async {
          try {
            await cubit.unlinkDevice(clientId: device.clientId);
          } catch (_) {
            if (!context.mounted) {
              return;
            }
            _showErrorDialog(
              context,
              title: loc.linkedDevicesScreen_unlinkError_title,
              message: loc.linkedDevicesScreen_unlinkError(name),
            );
          }
        },
      ),
    );
  }

  void _editDeviceName(BuildContext context, String currentName) {
    final loc = AppLocalizations.of(context);
    final cubit = context.read<LinkedDevicesCubit>();
    final navigator = Navigator.of(context);
    showDialog(
      context: context,
      builder: (_) => EditDialog(
        title: loc.linkedDevicesScreen_deviceName_editDialog_title,
        cancel: loc.linkedDevicesScreen_deviceName_editDialog_cancel,
        confirm: loc.linkedDevicesScreen_deviceName_editDialog_confirm,
        initialValue: currentName,
        maxLength: 30,
        validator: (value) => value.trim().isNotEmpty,
        onSubmit: (value) async {
          navigator.pop();
          try {
            await cubit.renameDevice(clientId: device.clientId, name: value);
          } catch (_) {
            if (!context.mounted) {
              return;
            }
            _showErrorDialog(
              context,
              title: loc.linkedDevicesScreen_renameError_title,
              // The old name: the rename did not take effect.
              message: loc.linkedDevicesScreen_renameError(currentName),
            );
          }
        },
      ),
    );
  }

  /// Reports a failed device action, with only a dismiss button: there is
  /// nothing to confirm, the action simply did not happen.
  void _showErrorDialog(
    BuildContext context, {
    required String title,
    required String message,
  }) {
    showDialog(
      context: context,
      builder: (_) => ConfirmDialog(
        title: title,
        message: message,
        confirm: AppLocalizations.of(
          context,
        ).linkedDevicesScreen_errorDialog_confirm,
      ),
    );
  }
}
