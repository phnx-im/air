// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:image_picker/image_picker.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/util/debouncer.dart';
import 'package:air/widgets/widgets.dart';
import 'package:provider/provider.dart';

const _listIconSize = 24.0;

class UserSettingsScreen extends StatelessWidget {
  const UserSettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final profile = context.select((UsersCubit cubit) => cubit.state.profile());

    final loc = AppLocalizations.of(context);

    final isMobilePlatform = Platform.isAndroid || Platform.isIOS;
    final isDesktopPlatform =
        Platform.isMacOS || Platform.isWindows || Platform.isLinux;

    return Scaffold(
      appBar: AppBar(
        title: Text(loc.userSettingsScreen_title),
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        minimum: const EdgeInsets.only(bottom: Spacings.m),
        child: Align(
          alignment: Alignment.topCenter,
          child: Container(
            constraints:
                isPointer() ? const BoxConstraints(maxWidth: 800) : null,
            child: SingleChildScrollView(
              child: Column(
                children: [
                  UserAvatar(
                    displayName: profile.displayName,
                    size: 100,
                    image: profile.profilePicture,
                    onPressed: () => _pickAvatar(context),
                  ),
                  const SizedBox(height: Spacings.xs),

                  const _UserProfileData(),

                  const SettingsDivider(),

                  const _UserHandles(),

                  const SettingsDivider(),

                  const _CommonSettings(),
                  if (isMobilePlatform) const _MobileSettings(),
                  if (isDesktopPlatform) const _DesktopSettings(),

                  const SettingsDivider(),

                  const _Help(),
                  const SizedBox(height: Spacings.xs),
                  const _DeleteAccount(),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _pickAvatar(BuildContext context) async {
    final user = context.read<UserCubit>();

    final ImagePicker picker = ImagePicker();
    final XFile? image = await picker.pickImage(source: ImageSource.gallery);
    final bytes = await image?.readAsBytes();

    if (bytes != null) {
      await user.setProfile(profilePicture: bytes);
    }
  }
}

class _UserProfileData extends StatelessWidget {
  const _UserProfileData();

  @override
  Widget build(BuildContext context) {
    final displayName = context.select(
      (UsersCubit cubit) => cubit.state.displayName(),
    );

    final loc = AppLocalizations.of(context);

    return ListView(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      children: [
        ListTile(
          leading: const Icon(Icons.person_outline, size: _listIconSize),
          title: Text(displayName),
          onTap:
              () => context.read<NavigationCubit>().openUserSettings(
                screen: UserSettingsScreenType.editDisplayName,
              ),
        ),
        const SizedBox(height: Spacings.xs),
        ListTile(
          subtitle: Text(
            style: TextStyle(color: Theme.of(context).hintColor),
            loc.userSettingsScreen_profileDescription,
          ),
        ),
      ],
    );
  }
}

class _UserHandles extends StatelessWidget {
  const _UserHandles();

  @override
  Widget build(BuildContext context) {
    final userHandles = context.select(
      (UserCubit cubit) => cubit.state.userHandles,
    );

    final loc = AppLocalizations.of(context);

    return ListView(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      children: [
        ...userHandles.expand(
          (handle) => [
            _UserHandle(handle: handle),
            const SizedBox(height: Spacings.xs),
          ],
        ),
        if (userHandles.isEmpty || userHandles.length < 5) ...[
          const _UserHandlePlaceholder(),
          const SizedBox(height: Spacings.xs),
        ],
        ListTile(
          subtitle: Text(
            style: TextStyle(color: Theme.of(context).hintColor),
            loc.userSettingsScreen_userNamesDescription,
          ),
        ),
      ],
    );
  }
}

class _UserHandle extends StatelessWidget {
  const _UserHandle({required this.handle});

  final UiUserHandle handle;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      leading: const Icon(Icons.alternate_email, size: _listIconSize),
      title: Text(handle.plaintext),
      onTap: () => _removeHandle(context),
    );
  }

  void _removeHandle(BuildContext context) async {
    final loc = AppLocalizations.of(context);
    await showDialog(
      context: context,
      builder: (BuildContext context) {
        return AlertDialog(
          title: Text(loc.removeUsernameDialog_title),
          content: Text(loc.removeUsernameDialog_content),
          actions: [
            TextButton(
              onPressed: () {
                Navigator.of(context).pop(false);
              },
              child: Text(loc.removeUsernameDialog_cancel),
            ),
            TextButton(
              onPressed: () async {
                await context.read<UserCubit>().removeUserHandle(handle);
                if (context.mounted) {
                  Navigator.of(context).pop(true);
                }
              },
              child: Text(loc.removeUsernameDialog_remove),
            ),
          ],
        );
      },
    );
  }
}

class _UserHandlePlaceholder extends StatelessWidget {
  const _UserHandlePlaceholder();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ListTile(
      leading: const Icon(Icons.alternate_email, size: _listIconSize),
      title: Text(
        style: TextStyle(color: Theme.of(context).hintColor),
        loc.userSettingsScreen_userHandlePlaceholder,
      ),
      onTap:
          () => context.read<NavigationCubit>().openUserSettings(
            screen: UserSettingsScreenType.addUserHandle,
          ),
    );
  }
}

class _CommonSettings extends StatefulWidget {
  const _CommonSettings();

  @override
  State<_CommonSettings> createState() => _CommonSettingsState();
}

class _CommonSettingsState extends State<_CommonSettings> {
  final Debouncer _readReceiptsDebouncer = Debouncer(
    delay: const Duration(milliseconds: 500),
  );
  bool _readReceipts = false;

  @override
  void initState() {
    super.initState();
    setState(() {
      _readReceipts = context.read<UserSettingsCubit>().state.readReceipts;
    });
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return ListView(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      children: [
        SwitchListTile(
          title: Text(loc.userSettingsScreen_readReceipts),
          subtitle: Text(
            style: TextStyle(color: Theme.of(context).hintColor),
            loc.userSettingsScreen_readReceiptsDescription,
          ),
          value: _readReceipts,
          onChanged: (value) {
            _readReceiptsDebouncer.run(() {
              context.read<UserSettingsCubit>().setReadReceipts(
                userCubit: context.read(),
                value: value,
              );
            });
            setState(() {
              _readReceipts = value;
            });
          },
        ),
      ],
    );
  }
}

class _MobileSettings extends StatefulWidget {
  const _MobileSettings();

  @override
  State<_MobileSettings> createState() => _MobileSettingsState();
}

class _MobileSettingsState extends State<_MobileSettings> {
  final Debouncer _sendOnEnterDebouncer = Debouncer(
    delay: const Duration(milliseconds: 500),
  );
  bool _sendOnEnter = false;

  @override
  void initState() {
    super.initState();
    setState(() {
      _sendOnEnter = context.read<UserSettingsCubit>().state.sendOnEnter;
    });
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return ListView(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      children: [
        SwitchListTile(
          title: Text(loc.userSettingsScreen_sendWithEnter),
          value: _sendOnEnter,
          onChanged: (value) {
            _sendOnEnterDebouncer.run(() {
              context.read<UserSettingsCubit>().setSendOnEnter(
                userCubit: context.read(),
                value: value,
              );
            });
            setState(() {
              _sendOnEnter = value;
            });
          },
        ),
      ],
    );
  }
}

class _DesktopSettings extends StatefulWidget {
  const _DesktopSettings();

  @override
  State<_DesktopSettings> createState() => _DesktopSettingsState();
}

class _DesktopSettingsState extends State<_DesktopSettings> {
  double _interfaceScaleSliderValue = 100.0;

  @override
  void initState() {
    super.initState();

    setState(() {
      final interfaceScale =
          context.read<UserSettingsCubit>().state.interfaceScale;
      var isLinuxAndScaled =
          Platform.isLinux &&
          WidgetsBinding.instance.platformDispatcher.textScaleFactor >= 1.5;
      _interfaceScaleSliderValue =
          100 * (interfaceScale ?? (isLinuxAndScaled ? 1.5 : 1.0));
    });
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ListView(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      children: [
        ListTile(
          leading: const Icon(Icons.visibility, size: _listIconSize),
          titleAlignment: ListTileTitleAlignment.top,
          title: Text(loc.userSettingsScreen_interfaceScale),
          subtitle: Slider(
            min: 50,
            max: 300,
            divisions: ((300 - 50) / 5).truncate(),
            value: _interfaceScaleSliderValue,
            label: _interfaceScaleSliderValue.truncate().toString(),
            activeColor: CustomColorScheme.of(context).text.secondary,
            onChanged:
                (value) => setState(() => _interfaceScaleSliderValue = value),
            onChangeEnd: (value) {
              context.read<UserSettingsCubit>().setInterfaceScale(
                userCubit: context.read(),
                value: value / 100,
              );
            },
          ),
        ),
      ],
    );
  }
}

Color getColor(Set<WidgetState> states) {
  const Set<WidgetState> interactiveStates = <WidgetState>{
    WidgetState.pressed,
    WidgetState.hovered,
    WidgetState.focused,
  };
  if (states.any(interactiveStates.contains)) {
    return Colors.brown;
  }
  return Colors.transparent;
}

class _Help extends StatelessWidget {
  const _Help();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ListView(
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      children: [
        ListTile(
          leading: const Icon(Icons.help, size: _listIconSize),
          title: Text(loc.userSettingsScreen_help),
          onTap: () {
            context.read<NavigationCubit>().openUserSettings(
              screen: UserSettingsScreenType.help,
            );
          },
        ),
      ],
    );
  }
}

class _DeleteAccount extends StatelessWidget {
  const _DeleteAccount();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final color = CustomColorScheme.of(context).function.danger;
    return ListTile(
      leading: const Icon(Icons.person_outline, size: _listIconSize),
      title: Text(loc.userSettingsScreen_deleteAccount),
      iconColor: color,
      textColor: color,
      onTap: () {
        context.read<NavigationCubit>().openUserSettings(
          screen: UserSettingsScreenType.deleteAccount,
        );
      },
    );
  }
}

class SettingsDivider extends StatelessWidget {
  const SettingsDivider({super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Spacings.xs),
      child: Divider(color: Theme.of(context).hintColor),
    );
  }
}
