// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icon.dart';
import 'package:air/user/user.dart';
import 'package:air/util/debouncer.dart';
import 'package:air/widgets/widgets.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:provider/provider.dart';

import 'add_username_dialog.dart';
import 'change_display_name_dialog.dart';
import 'contact_us_screen.dart';
import 'delete_account_dialog.dart';
import 'remove_username_dialog.dart';

class UserSettingsScreen extends StatelessWidget {
  const UserSettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final isMobilePlatform = Platform.isAndroid || Platform.isIOS;
    final isDesktopPlatform =
        Platform.isMacOS || Platform.isWindows || Platform.isLinux;

    final colors = CustomColorScheme.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(
          loc.userSettingsScreen_title,
          style: TextStyle(
            fontSize: LabelFontSize.base.size,
            fontWeight: FontWeight.bold,
          ),
        ),
        leading: AppBarBackButton(
          backgroundColor: colors.backgroundElevated.primary,
        ),
        actions: null,
        backgroundColor: Colors.transparent,
        toolbarHeight: isPointer() ? 100 : null,
        centerTitle: true,
        scrolledUnderElevation: 0,
      ),
      backgroundColor: colors.backgroundBase.secondary,
      body: SafeArea(
        child: SingleChildScrollView(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacings.s),
            child: Align(
              alignment: Alignment.topCenter,
              child: Container(
                constraints: isPointer()
                    ? const BoxConstraints(maxWidth: 800)
                    : null,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const SizedBox(height: 18),

                    const _UserAvatar(),

                    const SizedBox(height: Spacings.xs),
                    const _DisplayName(),

                    const SizedBox(height: Spacings.m),
                    const _UsernamesSection(),

                    const SizedBox(height: Spacings.m),
                    _SectionHeader(
                      text: loc.userSettingsScreen_settingsSection,
                    ),

                    const SizedBox(height: Spacings.xs),
                    const _CommonSettings(),

                    if (isMobilePlatform) const SizedBox(height: Spacings.xs),
                    if (isMobilePlatform) _MobileSettings(),

                    if (isDesktopPlatform) const SizedBox(height: Spacings.xs),
                    if (isDesktopPlatform) const _DesktopSettings(),

                    const SizedBox(height: Spacings.m),
                    const _HelpSection(),

                    const SizedBox(height: Spacings.m),
                    const _AccountSection(),

                    const SizedBox(height: Spacings.l + Spacings.xxs),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _UserAvatar extends StatelessWidget {
  const _UserAvatar();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: UserAvatar(size: 96, onPressed: () => _pickAvatar(context)),
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

class _DisplayName extends StatelessWidget {
  const _DisplayName();

  @override
  Widget build(BuildContext context) {
    String displayName;
    try {
      displayName = context.select(
        (UsersCubit cubit) => cubit.state.displayName(),
      );
    } on ProviderNotFoundException {
      return const SizedBox.shrink();
    }

    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        FieldLabel(loc.userSettingsScreen_displayNameLabel),

        const SizedBox(height: Spacings.xs),

        _FieldContainer(
          onTap: () => {
            showDialog(
              context: context,
              builder: (context) =>
                  ChangeDisplayNameDialog(displayName: displayName),
            ),
          },
          child: Row(children: [Text(displayName)]),
        ),

        const SizedBox(height: Spacings.xs),

        FieldLabel(loc.userSettingsScreen_profileDescription),
      ],
    );
  }
}

class _UsernamesSection extends StatelessWidget {
  const _UsernamesSection();

  @override
  Widget build(BuildContext context) {
    List<UiUserHandle> userHandles;
    try {
      userHandles = context.select(
        (UserCubit cubit) => cubit.state.userHandles,
      );
    } on ProviderNotFoundException {
      return const SizedBox.shrink();
    }

    final loc = AppLocalizations.of(context);

    final colors = CustomColorScheme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionHeader(text: loc.userSettingsScreen_usernamesSection),

        ...userHandles.expand(
          (handle) => [
            const SizedBox(height: Spacings.xs),
            _FieldContainer(
              child: Row(
                children: [
                  Text(handle.plaintext),
                  const Spacer(),
                  InkWell(
                    onTap: () {
                      showDialog(
                        context: context,
                        builder: (context) =>
                            RemoveUsernameDialog(username: handle),
                      );
                    },
                    child: AppIcon(
                      type: AppIconType.trash,
                      size: 24,
                      color: colors.function.danger,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),

        if (userHandles.isEmpty || userHandles.length < 5) ...[
          const SizedBox(height: Spacings.xs),
          _FieldContainer(
            onTap: () => showDialog(
              context: context,
              builder: (context) => const AddUsernameDialog(),
            ),
            child: Row(
              children: [
                Text(
                  loc.userSettingsScreen_userHandlePlaceholder,
                  style: TextStyle(color: colors.text.quaternary),
                ),
              ],
            ),
          ),

          const SizedBox(height: Spacings.xs),
          FieldLabel(loc.userSettingsScreen_userNamesDescription),
        ],
      ],
    );
  }
}

class _CommonSettings extends HookWidget {
  const _CommonSettings();

  @override
  Widget build(BuildContext context) {
    final readReceipts = useState(
      useMemoized(() => context.read<UserSettingsCubit>().state.readReceipts),
    );

    final loc = AppLocalizations.of(context);
    return Column(
      children: [
        _SwitchField(
          onSubmit: (value) {
            context.read<UserSettingsCubit>().setReadReceipts(
              userCubit: context.read(),
              value: value,
            );
          },
          value: readReceipts,
          label: loc.userSettingsScreen_readReceipts,
        ),

        const SizedBox(height: Spacings.xs),

        FieldLabel(loc.userSettingsScreen_readReceiptsDescription),
      ],
    );
  }
}

class _MobileSettings extends HookWidget {
  @override
  Widget build(BuildContext context) {
    final sendOnEnter = useState(
      useMemoized(() => context.read<UserSettingsCubit>().state.sendOnEnter),
    );

    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SwitchField(
          label: loc.userSettingsScreen_sendWithEnter,
          value: sendOnEnter,
          onSubmit: (value) {
            context.read<UserSettingsCubit>().setSendOnEnter(
              userCubit: context.read(),
              value: value,
            );
          },
        ),

        const SizedBox(height: Spacings.xs),

        FieldLabel(loc.userSettingsScreen_sendWithEnterDescription),
      ],
    );
  }
}

class _DesktopSettings extends HookWidget {
  const _DesktopSettings();

  @override
  Widget build(BuildContext context) {
    final interfaceScale = useState(
      useMemoized(() {
        final value = context.read<UserSettingsCubit>().state.interfaceScale;
        var isLinuxAndScaled =
            Platform.isLinux &&
            WidgetsBinding.instance.platformDispatcher.textScaleFactor >= 1.5;
        return 100 * (value ?? (isLinuxAndScaled ? 1.5 : 1.0));
      }),
    );

    final loc = AppLocalizations.of(context);

    return _FieldContainer(
      height: null,
      child: Row(
        children: [
          Text(
            loc.userSettingsScreen_interfaceScale,
            style: TextStyle(fontSize: BodyFontSize.base.size),
          ),
          const SizedBox(width: Spacings.xs),
          Expanded(
            child: Slider(
              min: 50,
              max: 300,
              divisions: ((300 - 50) / 10).truncate(),
              value: interfaceScale.value,
              label: interfaceScale.value.truncate().toString(),
              activeColor: CustomColorScheme.of(context).text.secondary,
              onChanged: (value) => interfaceScale.value = value,
              onChangeEnd: (value) {
                context.read<UserSettingsCubit>().setInterfaceScale(
                  userCubit: context.read(),
                  value: value / 100,
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _HelpSection extends HookWidget {
  const _HelpSection();

  @override
  Widget build(BuildContext context) {
    final packageInfoFut = useMemoized(() => PackageInfo.fromPlatform());
    final packageInfo = useFuture(packageInfoFut);

    final version = switch (packageInfo.data) {
      final info? => "${info.version}-${info.buildNumber}",
      null => "",
    };

    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionHeader(text: loc.userSettingsScreen_helpSection),

        const SizedBox(height: Spacings.xs),
        _FieldContainer(
          onTap: () {
            Navigator.of(context).push(
              MaterialPageRoute(builder: (context) => const ContactUsScreen()),
            );
          },
          child: Row(
            children: [
              Text(
                loc.helpScreen_contactUs,
                style: TextStyle(fontSize: BodyFontSize.base.size),
              ),
            ],
          ),
        ),

        const SizedBox(height: Spacings.xs),
        _FieldContainer(
          onTap: () {
            // copy to clipboard
            Clipboard.setData(ClipboardData(text: version));
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text(loc.settingsScreen_copiedToClipboard)),
            );
          },
          child: Row(
            children: [
              Text(
                loc.helpScreen_versionInfo,
                style: TextStyle(fontSize: BodyFontSize.base.size),
              ),
              const Spacer(),
              Text(version, style: TextStyle(fontSize: BodyFontSize.base.size)),
            ],
          ),
        ),

        const SizedBox(height: Spacings.xs),
        _FieldContainer(
          onTap: () {
            Navigator.of(context).push(
              MaterialPageRoute(builder: (context) => const LicensePage()),
            );
          },
          child: Row(
            children: [
              Text(
                loc.helpScreen_licenses,
                style: TextStyle(fontSize: BodyFontSize.base.size),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _AccountSection extends StatelessWidget {
  const _AccountSection();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionHeader(text: loc.userSettingsScreen_accountSection),

        const SizedBox(height: Spacings.xs),
        _FieldContainer(
          onTap: () {
            showDialog(
              context: context,
              builder: (context) => const DeleteAccountDialog(),
            );
          },
          child: Row(
            children: [
              Text(
                loc.userSettingsScreen_deleteAccount,
                style: TextStyle(
                  fontSize: BodyFontSize.base.size,
                  color: CustomColorScheme.of(context).function.danger,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class FieldLabel extends StatelessWidget {
  const FieldLabel(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
      child: Text(
        text,
        style: TextStyle(
          fontSize: LabelFontSize.small2.size,
          color: CustomColorScheme.of(context).text.quaternary,
        ),
      ),
    );
  }
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
      child: Text(
        text,
        style: TextStyle(
          fontSize: LabelFontSize.base.size,
          color: CustomColorScheme.of(context).text.secondary,
          fontWeight: FontWeight.bold,
        ),
      ),
    );
  }
}

class _SwitchField extends HookWidget {
  const _SwitchField({
    required this.onSubmit,
    required this.value,
    required this.label,
  });

  final Function(bool) onSubmit;
  final ValueNotifier<bool> value;
  final String label;

  @override
  Widget build(BuildContext context) {
    final debouncer = useMemoized(
      () => Debouncer(delay: const Duration(milliseconds: 500)),
    );
    useEffect(() => debouncer.dispose, []);

    final handleTap = useCallback(() {
      debouncer.run(() {
        onSubmit(value.value);
      });
      value.value = !value.value;
    }, [onSubmit, value]);

    return _FieldContainer(
      onTap: handleTap,
      child: Row(
        children: [
          Text(label, style: TextStyle(fontSize: BodyFontSize.base.size)),
          const Spacer(),
          Switch(
            value: value.value,
            padding: const EdgeInsets.symmetric(horizontal: 0),
            onChanged: (value) => handleTap(),
          ),
        ],
      ),
    );
  }
}

class _FieldContainer extends StatelessWidget {
  const _FieldContainer({required this.child, this.height = 42, this.onTap});

  final Widget child;
  final double? height;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return DefaultTextStyle(
      style: Theme.of(context).textTheme.bodyLarge!.copyWith(
        color: colors.text.primary,
        fontSize: BodyFontSize.base.size,
      ),
      child: InkWell(
        onTap: onTap,
        child: Container(
          decoration: BoxDecoration(
            color: colors.backgroundBase.tertiary,
            borderRadius: BorderRadius.circular(Spacings.s),
          ),
          padding: const EdgeInsets.symmetric(horizontal: Spacings.xs),
          height: height,
          child: child,
        ),
      ),
    );
  }
}
