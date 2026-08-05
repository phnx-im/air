// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:air/core/core.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/platform/method_channel.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:provider/provider.dart';

import 'package:air/features/developer/user_debug_info_panel.dart';

class DeveloperSettingsScreen extends StatefulWidget {
  const DeveloperSettingsScreen({super.key});

  @override
  State<DeveloperSettingsScreen> createState() =>
      _DeveloperSettingsScreenState();
}

class _DeveloperSettingsScreenState extends State<DeveloperSettingsScreen> {
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
  build(BuildContext context) {
    return DeveloperSettingsScreenView(
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

class DeveloperSettingsScreenView extends StatelessWidget {
  const DeveloperSettingsScreenView({
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
    final profile = user != null
        ? context.select(
            (UsersCubit cubit) => cubit.state.profile(userId: user.userId),
          )
        : null;

    final isDeveloper = context.select(
      (UserSettingsCubit cubit) => cubit.state.isDeveloper,
    );

    final isFrutigerAero = context.select(
      (UserSettingsCubit cubit) => cubit.state.frutigerAeroTheme,
    );

    return Scaffold(
      appBar: AppBar(
        clipBehavior: Clip.none,
        title: const Text('Developer Settings'),
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        child: Center(
          child: Container(
            constraints: DeviceType.isDesktop
                ? const BoxConstraints(maxWidth: 800)
                : null,
            child: ListTileTheme(
              data: Theme.of(context).listTileTheme.copyWith(
                titleAlignment: ListTileTitleAlignment.titleHeight,
                titleTextStyle: Theme.of(context).textTheme.bodyLarge!,
              ),
              child: ListView(
                children: [
                  const _SectionHeader("Developer mode"),
                  SwitchListTile(
                    title: const Text("Enable experimental features"),
                    value: isDeveloper,
                    onChanged: (value) {
                      context.read<UserSettingsCubit>().setIsDeveloper(
                        value: value,
                      );
                    },
                  ),
                  SwitchListTile(
                    title: const Text("Frutiger Aero theme"),
                    subtitle: const Text(
                      "Windows XP Luna blue, green Start button and all",
                    ),
                    value: isFrutigerAero,
                    onChanged: (value) {
                      context.read<UserSettingsCubit>().setFrutigerAeroTheme(
                        value: value,
                      );
                    },
                  ),
                  if (isMobile) ...[
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
                              onPressed: deviceToken == null
                                  ? null
                                  : () {
                                      Clipboard.setData(
                                        ClipboardData(text: deviceToken!),
                                      );
                                      showSnackBarStandalone(
                                        (loc) => const SnackBar(
                                          content: Text('Device token copied'),
                                        ),
                                      );
                                    },
                            ),
                          ),
                          Tooltip(
                            message: 'Refresh',
                            child: ButtonIcon(
                              variant: ButtonIconVariant.plain,
                              icon: AppIconType.refreshCw,
                              iconSize: S.s24,
                              hitTargetSize: S.s48,
                              onPressed: onRefreshPushToken,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                  const _SectionHeader("User"),
                  ListTile(
                    title: const Text("Change User"),
                    trailing: const AppIcon.repeat(),
                    onTap: () =>
                        context.read<NavigationCubit>().openDeveloperSettings(
                          screen: DeveloperSettingsScreenType.changeUser,
                        ),
                  ),
                  if (user != null) ...[
                    ListTile(
                      title: const Text("Log Out"),
                      trailing: const AppIcon.logOut(),
                      onTap: () => context.read<CoreClient>().logout(),
                    ),
                  ],
                  const _SectionHeader("App Data"),
                  ListTile(
                    title: const Text("Logs"),
                    trailing: const AppIcon.fileText(),
                    onTap: () =>
                        context.read<NavigationCubit>().openDeveloperSettings(
                          screen: DeveloperSettingsScreenType.logs,
                        ),
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
                            context.read<CoreClient>().deleteUserDatabase(),
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
                        context.read<CoreClient>().deleteDatabase();
                        context.read<NavigationCubit>().openIntro();
                      },
                      label: "Are you sure you want to erase all databases?",
                      confirmLabel: "Erase",
                    ),
                  ),
                  if (user != null) ...[
                    const _SectionHeader("Debug Info"),
                    Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: S.s12,
                        vertical: S.s8,
                      ),
                      child: UserDebugInfoPanel(user: user),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
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
      padding: const EdgeInsets.symmetric(vertical: S.s8, horizontal: S.s12),
      child: Text(label, style: const TextStyle(fontWeight: FontWeight.bold)),
    );
  }
}
