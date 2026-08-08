// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/features/developer/change_user_modal.dart';
import 'package:air/features/developer/logs_modal.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/method_channel.dart';
import 'package:provider/provider.dart';

import 'package:air/features/developer/user_debug_info_panel.dart';

/// The developer settings as a screen, for before a user is loaded.
class DeveloperSettingsScreen extends StatelessWidget {
  const DeveloperSettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return AppScaffold(
      title: AppLocalizations.of(context).youSection_developer,
      child: const SingleChildScrollView(child: DeveloperSettingsContent()),
    );
  }
}

/// The developer settings section. Sized and scrolled by its host.
class DeveloperSettingsContent extends StatefulWidget {
  const DeveloperSettingsContent({super.key});

  @override
  State<DeveloperSettingsContent> createState() =>
      _DeveloperSettingsContentState();
}

class _DeveloperSettingsContentState extends State<DeveloperSettingsContent> {
  String? deviceToken;

  @override
  void initState() {
    super.initState();
    _loadDeviceToken();
  }

  void _loadDeviceToken() async {
    final token = await getDeviceToken();
    setState(() {
      deviceToken = token;
    });
  }

  @override
  Widget build(BuildContext context) {
    return DeveloperSettingsView(
      deviceToken: deviceToken,
      isMobile: DeviceType.isPhone,
      onRefreshPushToken: () =>
          _reRegisterPushToken(context.read<CoreClient>()),
    );
  }

  void _reRegisterPushToken(CoreClient coreClient) async {
    final newDeviceToken = await getDeviceToken();
    if (newDeviceToken != null) {
      if (Platform.isAndroid) {
        final pushToken = PlatformPushToken.google(newDeviceToken);
        coreClient.user.updatePushToken(pushToken);
        setState(() {
          deviceToken = pushToken.token;
        });
      } else if (Platform.isIOS) {
        final pushToken = PlatformPushToken.apple(newDeviceToken);
        coreClient.user.updatePushToken(pushToken);
        setState(() {
          deviceToken = pushToken.token;
        });
      } else {
        throw StateError("unsupported platform");
      }
    }
  }
}

class DeveloperSettingsView extends StatelessWidget {
  const DeveloperSettingsView({
    required this.deviceToken,
    required this.onRefreshPushToken,
    required this.isMobile,
    super.key,
  });

  final String? deviceToken;
  final bool isMobile;
  final VoidCallback onRefreshPushToken;

  @override
  Widget build(BuildContext context) {
    final user = context.select(
      (LoadableUserCubit cubit) => cubit.state.loadedUser,
    );

    // These rows are Material list tiles, so they need an ink surface of
    // their own: the host paints its background over the nearest Material.
    return Material(
      type: MaterialType.transparency,
      child: ListTileTheme(
        data: Theme.of(context).listTileTheme.copyWith(
          titleAlignment: ListTileTitleAlignment.titleHeight,
          titleTextStyle: Theme.of(context).textTheme.bodyLarge!,
          // The host insets the section, so the tiles add none.
          contentPadding: EdgeInsets.zero,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const _DeveloperModeSection(),
            if (isMobile)
              _PushTokenSection(
                deviceToken: deviceToken,
                onRefresh: onRefreshPushToken,
              ),
            _UserSection(user: user),
            _AppDataSection(user: user),
            if (user != null) _DebugInfoSection(user: user),
          ],
        ),
      ),
    );
  }
}

class _DeveloperModeSection extends StatelessWidget {
  const _DeveloperModeSection();

  @override
  Widget build(BuildContext context) {
    final isDeveloper = context.select(
      (UserSettingsCubit cubit) => cubit.state.isDeveloper,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionHeader("Developer mode"),
        SwitchListTile(
          title: const Text("Enable experimental features"),
          value: isDeveloper,
          onChanged: (value) {
            context.read<UserSettingsCubit>().setIsDeveloper(value: value);
          },
        ),
      ],
    );
  }
}

class _PushTokenSection extends StatelessWidget {
  const _PushTokenSection({required this.deviceToken, required this.onRefresh});

  final String? deviceToken;
  final VoidCallback onRefresh;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionHeader("Mobile Device"),
        ListTile(
          title: const Text('Push Token'),
          subtitle: Text(deviceToken ?? "N/A"),
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Tooltip(
                message: 'Copy',
                child: ButtonIcon(
                  variant: ButtonIconVariant.plain,
                  icon: AppIconType.copy,
                  iconSize: S.s24,
                  hitTargetSize: S.s48,
                  onPressed: deviceToken == null ? null : _copyToken,
                ),
              ),
              Tooltip(
                message: 'Refresh',
                child: ButtonIcon(
                  variant: ButtonIconVariant.plain,
                  icon: AppIconType.refreshCw,
                  iconSize: S.s24,
                  hitTargetSize: S.s48,
                  onPressed: onRefresh,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  void _copyToken() {
    Clipboard.setData(ClipboardData(text: deviceToken!));
    showSnackBarStandalone(
      (loc) => const SnackBar(content: Text('Device token copied')),
    );
  }
}

class _UserSection extends StatelessWidget {
  const _UserSection({required this.user});

  final User? user;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionHeader("User"),
        ListTile(
          title: const Text("Change User"),
          trailing: const AppIcon.repeat(),
          onTap: () => showChangeUser(context),
        ),
        if (user != null)
          ListTile(
            title: const Text("Log Out"),
            trailing: const AppIcon.logOut(),
            onTap: () => context.read<CoreClient>().logout(),
          ),
      ],
    );
  }
}

class _AppDataSection extends StatelessWidget {
  const _AppDataSection({required this.user});

  final User? user;

  @override
  Widget build(BuildContext context) {
    final user = this.user;
    final profile = user != null
        ? context.select(
            (UsersCubit cubit) => cubit.state.profile(userId: user.userId),
          )
        : null;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionHeader("App Data"),
        ListTile(
          title: const Text("Logs"),
          trailing: const AppIcon.fileText(),
          onTap: () => showLogs(context),
        ),
        if (user != null)
          ListTile(
            title: Text(
              profile?.displayName ?? user.userId.uuid.toString(),
              style: Theme.of(context).textTheme.bodyLarge?.copyWith(
                color: Primitive.chromatic(Hue.red, Shade.s500),
              ),
            ),
            subtitle: Text("${user.userId}"),
            trailing: const AppIcon.trash(),
            onTap: () => _confirmDialog(
              context: context,
              onConfirm: () =>
                  context.read<CoreClient>().deleteCurrentDatabase(),
              label: "Are you sure you want to erase the database?",
              confirmLabel: "Erase",
            ),
          ),
        ListTile(
          title: Text(
            'Erase All Databases',
            style: Theme.of(context).textTheme.bodyLarge?.copyWith(
              color: Primitive.chromatic(Hue.red, Shade.s500),
              fontWeight: FontWeight.bold,
            ),
          ),
          trailing: const AppIcon.trash(),
          onTap: () => _confirmDialog(
            context: context,
            onConfirm: () {
              context.read<CoreClient>().deleteAllDatabases();
              context.read<NavigationCubit>().openIntro();
            },
            label: "Are you sure you want to erase all databases?",
            confirmLabel: "Erase",
          ),
        ),
      ],
    );
  }
}

class _DebugInfoSection extends StatelessWidget {
  const _DebugInfoSection({required this.user});

  final User user;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _SectionHeader("Debug Info"),
        Padding(
          padding: const EdgeInsets.symmetric(vertical: S.s8),
          child: UserDebugInfoPanel(user: user),
        ),
      ],
    );
  }
}

void _confirmDialog({
  required BuildContext context,
  required void Function() onConfirm,
  required String label,
  required String confirmLabel,
}) {
  showDialog(
    context: context,
    builder: (_) => ConfirmDialog(
      title: 'Confirmation',
      message: label,
      cancel: 'Cancel',
      confirm: confirmLabel,
      onConfirm: onConfirm,
      destructive: true,
    ),
  );
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader(this.label);

  final String label;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: S.s8),
      child: Text(label, style: const TextStyle(fontWeight: FontWeight.bold)),
    );
  }
}
