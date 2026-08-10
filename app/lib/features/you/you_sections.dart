// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/developer/developer_settings_section.dart';
import 'package:air/features/navigation/navigation_state.dart';
import 'package:air/features/user/avatar.dart';
import 'package:air/features/user/loadable_user_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/features/you/add_username_dialog.dart';
import 'package:air/features/you/change_display_name_dialog.dart';
import 'package:air/features/you/contact_us_modal.dart';
import 'package:air/features/you/delete_account_dialog.dart';
import 'package:air/features/you/invitation_codes_cubit.dart';
import 'package:air/features/you/invitation_codes_modal.dart';
import 'package:air/features/you/linked_devices_screen.dart';
import 'package:air/features/you/remove_username_dialog.dart';
import 'package:air/features/you/you_fields.dart';
import 'package:air/l10n/language_picker_menu.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:logging/logging.dart';
import 'package:package_info_plus/package_info_plus.dart';

final _log = Logger('YouSections');

/// Title of a section, as shown in the header above its content.
String youSectionTitle(AppLocalizations loc, YouSection section) =>
    switch (section) {
      YouSection.profile => loc.youSection_profile,
      YouSection.devices => loc.userSettingsScreen_devices,
      YouSection.account => loc.userSettingsScreen_accountSection,
      YouSection.preferences => loc.youSection_preferences,
      YouSection.help => loc.userSettingsScreen_helpSection,
      YouSection.developer => loc.youSection_developer,
    };

/// The body of one profile section, shared by the two-pane detail pane and the
/// phone's pushed section screen. The header naming the section belongs to the
/// host, so a section never repeats its own title.
class YouSectionContent extends StatelessWidget {
  const YouSectionContent({super.key, required this.section});

  final YouSection section;

  @override
  Widget build(BuildContext context) => switch (section) {
    YouSection.profile => const ProfileSection(),
    YouSection.devices => const LinkedDevicesContent(),
    YouSection.account => const AccountSection(),
    YouSection.preferences => const PreferencesSection(),
    YouSection.help => const HelpSection(),
    YouSection.developer => const DeveloperSettingsContent(),
  };
}

/// Identity: profile picture, display name, and the usernames others can reach
/// you by.
class ProfileSection extends StatelessWidget {
  const ProfileSection({super.key});

  @override
  Widget build(BuildContext context) {
    return const Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _UserAvatar(),
        SizedBox(height: S.s12),
        _DisplayName(),
        SizedBox(height: S.s24),
        _UsernamesSection(),
      ],
    );
  }
}

class _UserAvatar extends StatelessWidget {
  const _UserAvatar();

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: null),
    );
    return Center(
      child: UserAvatar(
        profile: profile,
        size: S.s192,
        onPressed: () => _pickAvatar(context),
      ),
    );
  }

  void _pickAvatar(BuildContext context) async {
    final user = context.read<UserCubit>();

    final ImagePicker picker = ImagePicker();
    // Reduce image quality to re-encode the image.
    final XFile? image = await picker.pickImage(
      source: ImageSource.gallery,
      imageQuality: 99,
    );
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

        const SizedBox(height: S.s12),

        FieldContainer(
          onTap: () => {
            showDialog(
              context: context,
              builder: (context) =>
                  ChangeDisplayNameDialog(displayName: displayName),
            ),
          },
          child: Row(children: [Text(displayName)]),
        ),

        const SizedBox(height: S.s12),

        FieldLabel(loc.userSettingsScreen_profileDescription),
      ],
    );
  }
}

class _UsernamesSection extends StatelessWidget {
  const _UsernamesSection();

  @override
  Widget build(BuildContext context) {
    List<UiUsername> usernames;
    try {
      usernames = context.select((UserCubit cubit) => cubit.state.usernames);
    } on ProviderNotFoundException {
      return const SizedBox.shrink();
    }

    final loc = AppLocalizations.of(context);

    final palette = SemanticPalette.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionHeader(text: loc.userSettingsScreen_usernamesSection),

        ...usernames.expand(
          (username) => [
            const SizedBox(height: S.s12),
            FieldContainer(
              child: Row(
                children: [
                  Text(username.plaintext),
                  const Spacer(),
                  InkWell(
                    onTap: () {
                      showDialog(
                        context: context,
                        builder: (context) =>
                            RemoveUsernameDialog(username: username),
                      );
                    },
                    child: AppIcon.trash(
                      size: S.s24,
                      color: palette.function.danger,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),

        if (usernames.isEmpty || usernames.length < 5) ...[
          const SizedBox(height: S.s12),
          FieldContainer(
            onTap: () => showDialog(
              context: context,
              builder: (context) => const AddUsernameDialog(),
            ),
            child: Row(
              children: [
                Text(
                  loc.userSettingsScreen_usernamePlaceholder,
                  style: TextStyle(color: palette.text.quaternary),
                ),
              ],
            ),
          ),

          const SizedBox(height: S.s12),
          FieldLabel(loc.userSettingsScreen_userNamesDescription),
        ],
      ],
    );
  }
}

/// Account: what you can hand out, and what you can give up.
class AccountSection extends StatelessWidget {
  const AccountSection({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const _InviteCodes(),

        const SizedBox(height: S.s12),
        FieldContainer(
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
                style: typeScale.body.regular.style(
                  color: SemanticPalette.of(context).function.danger,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _InviteCodes extends StatelessWidget {
  const _InviteCodes();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    return FieldContainer(
      onTap: () => showInvitationCodes(context),
      child: Row(
        children: [
          AppIcon.users(color: palette.text.secondary, size: S.s24),

          const SizedBox(width: S.s12),

          Expanded(child: Text(loc.userSettingsScreen_inviteCodes)),

          const _InvitationCodesBadge(),
        ],
      ),
    );
  }
}

class _InvitationCodesBadge extends StatelessWidget {
  const _InvitationCodesBadge();

  @override
  Widget build(BuildContext context) {
    final availableInvitationCodes = context.select(
      (InvitationCodesCubit cubit) => cubit.state.codes
          .where(
            (code) => switch (code) {
              UiInvitationCode_Token() => true,
              UiInvitationCode_Code(field0: final code) => !code.copied,
            },
          )
          .length,
    );

    if (availableInvitationCodes == 0) {
      return const SizedBox.shrink();
    }

    final palette = SemanticPalette.of(context);

    return Container(
      width: S.s40,
      height: S.s24,
      decoration: BoxDecoration(
        color: palette.function.success.primary,
        borderRadius: BorderRadius.circular(CornerRadius.full),
      ),
      child: Center(
        child: Text(
          availableInvitationCodes.toString(),
          style: typeScale.body.xs.style(color: palette.function.neutral.white),
        ),
      ),
    );
  }
}

/// Preferences: the switches and pickers that shape day-to-day use.
class PreferencesSection extends HookWidget {
  const PreferencesSection({super.key});

  @override
  Widget build(BuildContext context) {
    // Captured here rather than read inside the submit callback: a debounced
    // submit can fire while the widget is being disposed, when context
    // lookups are no longer allowed.
    final settingsCubit = context.read<UserSettingsCubit>();
    final readReceiptsSetting = context.select(
      (UserSettingsCubit cubit) => cubit.state.readReceipts,
    );
    // The subscription above also provides the initial value: useState only
    // reads its argument on the first build.
    final readReceipts = useState(readReceiptsSetting);
    // Converge the local switch onto the cubit state. This moves the switch
    // for out-of-band changes (a sibling device update or a rollback) and
    // confirms it after a successful submit. The optimistic local flip
    // survives because the cubit state only changes on success.
    useEffect(() {
      readReceipts.value = readReceiptsSetting;
      return null;
    }, [readReceiptsSetting]);

    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: S.s12,
      children: [
        const _LanguageSettings(),
        SwitchField(
          onSubmit: (value) async {
            try {
              await settingsCubit.setReadReceipts(value: value);
            } catch (e) {
              // The submit failed, so the cubit state did not move. Revert the
              // optimistic local flip to match it.
              _log.severe("Failed to set read receipts: $e", e);
              // The flush on dispose submits a pending tap during unmount, so
              // this can fail after the notifier is gone. There is no UI left
              // to revert then, and writing to a disposed notifier throws.
              if (context.mounted) {
                readReceipts.value = settingsCubit.state.readReceipts;
              }
            }
          },
          value: readReceipts,
          label: loc.userSettingsScreen_readReceipts,
        ),
        FieldLabel(loc.userSettingsScreen_readReceiptsDescription),

        if (DeviceType.isPhone) const _SendOnEnterSetting(),
        if (DeviceType.isDesktop) const _InterfaceScaleSetting(),
      ],
    );
  }
}

class _LanguageSettings extends StatelessWidget {
  const _LanguageSettings();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return LanguagePickerMenu(
      onLocaleSelected: (locale) async {
        context.read<AppLocaleCubit>().setLocale(locale);
        // Before login there is no user to persist the locale to.
        if (context.read<LoadableUserCubit>().state.loadedUser == null) {
          return;
        }
        await context.read<UserSettingsCubit>().setLocale(
          value: locale.languageCode,
        );
      },
      childBuilder: (context, option, onTap) {
        return FieldContainer(
          onTap: onTap,
          child: Row(
            children: [
              AppIcon.globe(color: palette.text.secondary, size: S.s24),
              const SizedBox(width: S.s12),
              Expanded(child: Text(option.label)),
            ],
          ),
        );
      },
    );
  }
}

class _SendOnEnterSetting extends HookWidget {
  const _SendOnEnterSetting();

  @override
  Widget build(BuildContext context) {
    // Captured here rather than read inside the submit callback: a debounced
    // submit can fire while the widget is being disposed, when context
    // lookups are no longer allowed.
    final settingsCubit = context.read<UserSettingsCubit>();
    final sendOnEnter = useState(
      useMemoized(() => settingsCubit.state.sendOnEnter),
    );

    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SwitchField(
          label: loc.userSettingsScreen_sendWithEnter,
          value: sendOnEnter,
          onSubmit: (value) {
            settingsCubit.setSendOnEnter(value: value);
          },
        ),

        const SizedBox(height: S.s12),

        FieldLabel(loc.userSettingsScreen_sendWithEnterDescription),
      ],
    );
  }
}

class _InterfaceScaleSetting extends HookWidget {
  const _InterfaceScaleSetting();

  @override
  Widget build(BuildContext context) {
    // The slider carries the user's own factor, which systemInterfaceScale
    // multiplies rather than replaces, so it starts at 100% everywhere.
    final interfaceScale = useState(
      useMemoized(() {
        final value = context.read<UserSettingsCubit>().state.interfaceScale;
        return 100 * (value ?? 1.0);
      }),
    );

    final loc = AppLocalizations.of(context);

    return FieldContainer(
      height: null,
      child: Row(
        children: [
          Text(
            loc.userSettingsScreen_interfaceScale,
            style: typeScale.body.regular.style(),
          ),
          const SizedBox(width: S.s12),
          Expanded(
            child: Slider(
              min: 50,
              max: 300,
              divisions: ((300 - 50) / 10).truncate(),
              value: interfaceScale.value,
              label: interfaceScale.value.truncate().toString(),
              activeColor: SemanticPalette.of(context).text.secondary,
              onChanged: (value) => interfaceScale.value = value,
              onChangeEnd: (value) {
                context.read<UserSettingsCubit>().setInterfaceScale(
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

/// Help: reaching us, and telling us which build you are on.
class HelpSection extends HookWidget {
  const HelpSection({super.key});

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
        FieldContainer(
          onTap: () => showContactUs(context),
          child: Row(
            children: [
              Text(
                loc.helpScreen_contactUs,
                style: typeScale.body.regular.style(),
              ),
            ],
          ),
        ),

        const SizedBox(height: S.s12),
        FieldContainer(
          onTap: () {
            Clipboard.setData(ClipboardData(text: version));
            showSnackBarStandalone(
              (loc) =>
                  SnackBar(content: Text(loc.settingsScreen_copiedToClipboard)),
            );
          },
          child: Row(
            children: [
              Text(
                loc.helpScreen_versionInfo,
                style: typeScale.body.regular.style(),
              ),
              const Spacer(),
              Text(version, style: typeScale.body.regular.style()),
            ],
          ),
        ),

        const SizedBox(height: S.s12),
        FieldContainer(
          onTap: () {
            Navigator.of(context).push(
              MaterialPageRoute(builder: (context) => const LicensePage()),
            );
          },
          child: Row(
            children: [
              Text(
                loc.helpScreen_licenses,
                style: typeScale.body.regular.style(),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
