// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:air/core/core.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/ui/colors/palette.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/theme/theme.dart';
import 'package:air/util/platform.dart';
import 'package:air/widgets/widgets.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';
import 'package:share_plus/share_plus.dart';

final _log = Logger("DeveloperSettingsScreen");

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
      isMobile: Platform.isAndroid || Platform.isIOS,
      onRefreshPushToken:
          () => _reRegisterPushToken(context.read<CoreClient>()),
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
    final user = context.select((LoadableUserCubit cubit) => cubit.state.user);
    final profile =
        user != null
            ? context.select(
              (UsersCubit cubit) => cubit.state.profile(userId: user.userId),
            )
            : null;

    return Scaffold(
      appBar: AppBar(
        title: const Text('Developer Settings'),
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        child: Center(
          child: Container(
            constraints:
                isPointer() ? const BoxConstraints(maxWidth: 800) : null,
            child: ListTileTheme(
              data: Theme.of(context).listTileTheme.copyWith(
                titleAlignment: ListTileTitleAlignment.titleHeight,
                titleTextStyle: Theme.of(context).textTheme.bodyLarge!,
              ),
              child: ListView(
                children: [
                  if (isMobile) ...[
                    const _SectionHeader("Mobile Device"),
                    ListTile(
                      title: const Text('Push Token'),
                      subtitle: Text(deviceToken ?? "N/A"),
                      trailing: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          IconButton(
                            icon: const Icon(Icons.copy),
                            tooltip: 'Copy',
                            onPressed: deviceToken == null
                                ? null
                                : () {
                                    Clipboard.setData(
                                      ClipboardData(text: deviceToken!),
                                    );
                                    ScaffoldMessenger.of(context).showSnackBar(
                                      const SnackBar(
                                        content: Text('Device token copied'),
                                      ),
                                    );
                                  },
                          ),
                          IconButton(
                            icon: const Icon(Icons.refresh),
                            tooltip: 'Refresh',
                            onPressed: onRefreshPushToken,
                          ),
                        ],
                      ),
                    ),
                  ],
                  const _SectionHeader("User"),
                  ListTile(
                    title: const Text("Change User"),
                    trailing: const Icon(Icons.change_circle),
                    onTap:
                        () => context
                            .read<NavigationCubit>()
                            .openDeveloperSettings(
                              screen: DeveloperSettingsScreenType.changeUser,
                            ),
                  ),
                  if (user != null) ...[
                    ListTile(
                      title: const Text("Log Out"),
                      trailing: const Icon(Icons.logout),
                      onTap: () => context.read<CoreClient>().logout(),
                    ),
                  ],
                  const _SectionHeader("App Data"),
                  ListTile(
                    title: const Text("Logs"),
                    trailing: const Icon(Icons.text_snippet),
                    onTap:
                        () => context
                            .read<NavigationCubit>()
                            .openDeveloperSettings(
                              screen: DeveloperSettingsScreenType.logs,
                            ),
                  ),
                  if (user != null)
                    ListTile(
                      title: const Text("Export Database"),
                      trailing: const Icon(Icons.file_download),
                      onTap: () => _exportDatabase(context, user),
                    ),
                  ListTile(
                    title: const Text("Import Database"),
                    trailing: const Icon(Icons.file_upload),
                    onTap: () => _importDatabase(context),
                  ),
                  if (user != null)
                    ListTile(
                      title: Text(
                        profile?.displayName ?? user.userId.uuid.toString(),
                        style: Theme.of(
                          context,
                        ).textTheme.bodyLarge?.copyWith(color: AppColors.red),
                      ),
                      subtitle: Text("${user.userId}"),
                      trailing: const Icon(Icons.delete),
                      onTap:
                          () => _confirmDialog(
                            context: context,
                            onConfirm:
                                () =>
                                    context
                                        .read<CoreClient>()
                                        .deleteUserDatabase(),
                            label:
                                "Are you sure you want to erase the database?",
                            confirmLabel: "Erase",
                          ),
                    ),
                  ListTile(
                    title: Text(
                      'Erase All Databases',
                      style: Theme.of(context).textTheme.bodyLarge?.copyWith(
                        color: AppColors.red,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    trailing: const Icon(Icons.delete),
                    onTap:
                        () => _confirmDialog(
                          context: context,
                          onConfirm: () {
                            context.read<CoreClient>().deleteDatabase();
                            context.read<NavigationCubit>().openIntro();
                          },
                          label:
                              "Are you sure you want to erase all databases?",
                          confirmLabel: "Erase",
                        ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _exportDatabase(BuildContext context, User user) async {
    final isMobile = Platform.isAndroid || Platform.isIOS;
    if (isMobile) {
      // on mobile, the exported database is shared as a file
      final bytes = await exportClientDatabase(
        dbPath: await dbPath(),
        userId: user.userId,
      );
      final params = ShareParams(
        files: [XFile.fromData(bytes, mimeType: "application/gzip")],
        fileNameOverrides: ["${user.userId}.tar.gz"],
      );
      SharePlus.instance.share(params);
    } else {
      // on desktop, we ask for a directory and save the database there
      final location = await getSaveLocation(
        suggestedName: "${user.userId}.tar.gz",
      );
      if (location == null) {
        return;
      }
      _log.info("Exporting client database to ${location.path}");
      final bytes = await exportClientDatabase(
        dbPath: await dbPath(),
        userId: user.userId,
      );
      File(location.path).writeAsBytes(bytes);
    }
  }

  void _importDatabase(BuildContext context) async {
    final file = await openFile();
    if (file == null) {
      return;
    }
    final bytes = await file.readAsBytes();
    await importClientDatabase(dbPath: await dbPath(), tarGzBytes: bytes);
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
    builder: (BuildContext context) {
      return AlertDialog(
        title: const Text('Confirmation'),
        content: Text(label),
        actions: [
          TextButton(
            child: const Text('Cancel'),
            onPressed: () {
              Navigator.of(context).pop();
            },
          ),
          TextButton(
            style: TextButton.styleFrom(
              backgroundColor: AppColors.red,
              foregroundColor: CustomColorScheme.of(context).function.white,
            ),
            onPressed: onConfirm,
            child: Text(confirmLabel),
          ),
        ],
      );
    },
  );
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader(this.label);

  final String label;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(
        vertical: Spacings.xxs,
        horizontal: Spacings.xs,
      ),
      child: Text(label, style: const TextStyle(fontWeight: FontWeight.bold)),
    );
  }
}
