// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/components/toggle/toggle.dart';
import 'package:air/ds/components/toggle/toggle_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/developer/change_user_modal.dart';
import 'package:air/features/developer/developer_fields.dart';
import 'package:air/features/developer/logs_screen.dart';
import 'package:air/features/developer/user_debug_info.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_session_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/method_channel.dart';
import 'package:flutter/material.dart' show Slider;
import 'package:flutter/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

/// The developer settings as a screen, for before a user is loaded.
class DeveloperSettingsScreen extends StatelessWidget {
  const DeveloperSettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return AppScaffold(
      title: AppLocalizations.of(context).youSection_developer,
      child: const DeveloperSettingsContent(),
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
    if (!mounted) return;
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
    final pushToken = await getPushToken();
    if (pushToken == null) return;

    coreClient.user.updatePushToken(pushToken);

    if (!mounted) return;
    setState(() {
      deviceToken = pushToken.token;
    });
  }
}

/// The section as cards, ordered by what a row does to the app: tune, switch,
/// inspect, destroy. A new row joins the card its consequences put it in.
class DeveloperSettingsView extends HookWidget {
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
      (UserSessionCubit cubit) => cubit.state.activeUser,
    );

    final debugInfo = useUserDebugInfo(user);
    final info = debugInfo.info;

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: S.s16,
      children: [
        const _SettingsCard(),
        _SessionCard(user: user),
        _DiagnosticsCard(
          isMobile: isMobile,
          deviceToken: deviceToken,
          onRefreshPushToken: onRefreshPushToken,
          info: info,
          error: debugInfo.error,
        ),
        if (user != null && info != null)
          TimedTasksCard(
            user: user,
            tasks: info.timedTasks,
            onTriggered: debugInfo.refresh,
          ),
        _DangerZoneCard(user: user),
      ],
    );
  }
}

/// What the surface itself is set to.
class _SettingsCard extends StatelessWidget {
  const _SettingsCard();

  @override
  Widget build(BuildContext context) {
    final developerMode = context.select(
      (UserSettingsCubit cubit) => cubit.state.developerMode,
    );
    final experimentalFeatures = context.select(
      (UserSettingsCubit cubit) => cubit.state.experimentalFeatures,
    );
    final settings = context.read<UserSettingsCubit>();

    return DeveloperCard(
      children: [
        _ToggleRow(
          label: 'Developer mode',
          value: developerMode,
          onChanged: (value) => settings.setDeveloperMode(value: value),
        ),
        _ToggleRow(
          label: 'Experimental features',
          value: experimentalFeatures,
          // The switch sits inside developer mode, so locking the surface
          // greys it out rather than erasing its value.
          enabled: developerMode,
          onChanged: (value) => settings.setExperimentalFeatures(value: value),
        ),
        const _InterfaceScaleRow(),
      ],
    );
  }
}

/// Scales the whole interface, so a layout can be read at a size other than
/// the device's own.
class _InterfaceScaleRow extends HookWidget {
  const _InterfaceScaleRow();

  @override
  Widget build(BuildContext context) {
    final settings = context.read<UserSettingsCubit>();
    // The slider carries the user's own factor, which systemInterfaceScale
    // multiplies rather than replaces. It starts at 100% everywhere.
    final percent = useState(
      useMemoized(() => 100 * (settings.state.interfaceScale ?? 1.0)),
    );

    void submit(double value) {
      percent.value = value;
      settings.setInterfaceScale(value: value / 100);
    }

    final palette = SemanticPalette.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s8),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          Row(
            children: [
              Text(
                'Interface scale',
                style: typeScale.body.regular.style(
                  color: palette.text.primary,
                ),
              ),
              const Spacer(),
              Text(
                '${percent.value.round()}%',
                style: typeScale.body.s
                    .style(color: palette.text.tertiary)
                    .withSystemMonospace(),
              ),
              const SizedBox(width: S.s12),
              DeveloperRowButton(
                icon: AppIconType.refreshCcw,
                tooltip: 'Reset to 100%',
                onPressed: percent.value == 100 ? null : () => submit(100),
              ),
            ],
          ),
          Slider(
            min: 50,
            max: 300,
            divisions: ((300 - 50) / 10).truncate(),
            value: percent.value,
            activeColor: palette.text.secondary,
            onChanged: (value) => percent.value = value,
            onChangeEnd: submit,
          ),
        ],
      ),
    );
  }
}

/// What the app will tell you about itself.
class _DiagnosticsCard extends StatelessWidget {
  const _DiagnosticsCard({
    required this.isMobile,
    required this.deviceToken,
    required this.onRefreshPushToken,
    required this.info,
    required this.error,
  });

  final bool isMobile;
  final String? deviceToken;
  final VoidCallback onRefreshPushToken;

  /// The loaded user's debug info, null until it resolves. Its rows are absent
  /// until then, so the rest of the page renders right away.
  final UserDebugInfo? info;

  final Object? error;

  @override
  Widget build(BuildContext context) {
    final info = this.info;
    final error = this.error;

    return DeveloperCard(
      caption: 'Diagnostics',
      children: [
        ListRow(
          tokens: ListRowTokens.current,
          label: 'Logs',
          separator: false,
          trailing: const AppIcon.fileText(size: developerRowIconSize),
          onTap: () => showLogs(context),
        ),
        if (isMobile)
          DeveloperInfoRow(
            label: 'Push token',
            value: deviceToken ?? 'N/A',
            monospace: true,
            trailing: DeveloperRowButton(
              icon: AppIconType.refreshCw,
              tooltip: 'Refresh',
              onPressed: onRefreshPushToken,
            ),
          ),
        if (info != null) ...[
          DeveloperInfoRow(
            label: 'User ID',
            value: info.userId,
            monospace: true,
          ),
          DeveloperInfoRow(
            label: 'Add-username tokens',
            value: info.addUsernameTokenCount.toString(),
          ),
          DeveloperInfoRow(
            label: 'Invite-code tokens',
            value: info.invitationCodeTokenCount.toString(),
          ),
        ],
        if (error != null)
          DeveloperInfoRow(label: 'Debug info error', value: error.toString()),
      ],
    );
  }
}

/// Which user the app is running as.
class _SessionCard extends StatelessWidget {
  const _SessionCard({required this.user});

  final User? user;

  @override
  Widget build(BuildContext context) {
    final tokens = ListRowTokens.current;

    return DeveloperCard(
      caption: 'Session',
      children: [
        ListRow(
          tokens: tokens,
          label: 'Change user',
          separator: false,
          trailing: const AppIcon.repeat(size: developerRowIconSize),
          onTap: () => showChangeUser(context),
        ),
        if (user != null)
          ListRow(
            tokens: tokens,
            label: 'Log out',
            separator: false,
            trailing: const AppIcon.logOut(size: developerRowIconSize),
            onTap: () => context.read<CoreClient>().logout(),
          ),
      ],
    );
  }
}

/// What cannot be undone.
class _DangerZoneCard extends StatelessWidget {
  const _DangerZoneCard({required this.user});

  final User? user;

  @override
  Widget build(BuildContext context) {
    final user = this.user;
    final profile = user != null
        ? context.select(
            (UsersCubit cubit) => cubit.state.profile(userId: user.userId),
          )
        : null;

    return DeveloperCard(
      caption: 'Danger zone',
      children: [
        if (user != null)
          DeveloperDangerRow(
            label: 'Erase this database',
            sublabel: profile?.displayName ?? user.userId.uuid.toString(),
            icon: AppIconType.trash,
            confirmMessage: 'Are you sure you want to erase the database?',
            confirmLabel: 'Erase',
            onConfirm: () => context.read<CoreClient>().deleteCurrentDatabase(),
          ),
        DeveloperDangerRow(
          label: 'Erase all databases',
          icon: AppIconType.trash,
          confirmMessage: 'Are you sure you want to erase all databases?',
          confirmLabel: 'Erase',
          onConfirm: () {
            context.read<CoreClient>().deleteAllDatabases();
            context.read<NavigationCubit>().openIntro();
          },
        ),
      ],
    );
  }
}

/// A row whose whole width flips the switch it carries.
class _ToggleRow extends StatelessWidget {
  const _ToggleRow({
    required this.label,
    required this.value,
    required this.onChanged,
    this.enabled = true,
  });

  final String label;
  final bool value;
  final ValueChanged<bool> onChanged;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return ListRow(
      tokens: ListRowTokens.current,
      label: label,
      separator: false,
      labelStyle: enabled
          ? null
          : typeScale.body.regular.style(
              color: palette.text.tertiary,
              tight: true,
            ),
      trailing: Toggle(
        tokens: ToggleTokens.compact,
        value: value,
        enabled: enabled,
        onChanged: (_) => onChanged(!value),
      ),
      onTap: enabled ? () => onChanged(!value) : null,
    );
  }
}
