// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:air/util/debouncer.dart';
import 'package:air/widgets/widgets.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:provider/provider.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;

class UserSettingsScreen extends StatelessWidget {
  const UserSettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    UiUserProfile profile;
    try {
      profile = context.select((UsersCubit cubit) => cubit.state.profile());
    } on ProviderNotFoundException {
      return const SizedBox.shrink();
    }

    final loc = AppLocalizations.of(context);

    final isMobilePlatform = Platform.isAndroid || Platform.isIOS;
    final isDesktopPlatform =
        Platform.isMacOS || Platform.isWindows || Platform.isLinux;

    final isKeyboardShown = MediaQuery.viewInsetsOf(context).bottom > 0;

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
      ),
      backgroundColor: colors.backgroundBase.secondary,
      body: SafeArea(
        minimum: EdgeInsets.only(
          left: Spacings.s,
          right: Spacings.s,
          bottom: isKeyboardShown ? Spacings.s : Spacings.l + Spacings.xxs,
        ),
        child: Container(
          constraints: isPointer() ? const BoxConstraints(maxWidth: 800) : null,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SizedBox(height: Spacings.s + 2),
                Center(
                  child: UserAvatar(
                    displayName: profile.displayName,
                    size: 96,
                    image: profile.profilePicture,
                    onPressed: () => _pickAvatar(context),
                  ),
                ),
                const SizedBox(height: Spacings.xs),

                const _UserProfileData(),

                const SizedBox(height: Spacings.m),

                const _UserHandles(),

                const SizedBox(height: Spacings.m),

                _SectionHeader(text: loc.userSettingsScreen_settingsSection),

                const SizedBox(height: Spacings.xs),

                const _CommonSettings(),

                if (isMobilePlatform) const SizedBox(height: Spacings.xs),
                if (isMobilePlatform) _MobileSettings(),

                if (isDesktopPlatform) const SizedBox(height: Spacings.xs),
                if (isDesktopPlatform) const _DesktopSettings(),

                const SizedBox(height: Spacings.m),
                const _Help(),

                const SizedBox(height: Spacings.m),
                const _DeleteAccount(),
              ],
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
        _FormLabel(loc.userSettingsScreen_displayNameLabel),

        const SizedBox(height: Spacings.xs),

        _FieldContainer(
          onTap:
              () => {
                showDialog(
                  context: context,
                  builder:
                      (context) =>
                          _ChangeUsernameDialog(displayName: displayName),
                ),
              },
          child: Row(children: [Text(displayName)]),
        ),

        const SizedBox(height: Spacings.xs),

        _FormLabel(loc.userSettingsScreen_profileDescription),
      ],
    );
  }
}

class _UserHandles extends StatelessWidget {
  const _UserHandles();

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

        const SizedBox(height: Spacings.xs),

        ...userHandles.expand(
          (handle) => [
            _FieldContainer(
              child: Row(
                children: [
                  Text(handle.plaintext),
                  const Spacer(),
                  InkWell(
                    onTap: () {
                      showDialog(
                        context: context,
                        builder:
                            (context) =>
                                _RemoveUsernameDialog(username: handle),
                      );
                    },
                    child: iconoir.Trash(
                      width: 24,
                      height: 24,
                      color: colors.function.danger,
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: Spacings.xs),
          ],
        ),

        if (userHandles.isEmpty || userHandles.length < 5) ...[
          _FieldContainer(
            onTap:
                () => showDialog(
                  context: context,
                  builder: (context) => const _AddUsernameDialog(),
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

          _FormLabel(loc.userSettingsScreen_userNamesDescription),
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

        _FormLabel(loc.userSettingsScreen_readReceiptsDescription),
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

        _FormLabel(loc.userSettingsScreen_sendWithEnterDescription),
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
            child: Container(
              decoration: BoxDecoration(
                border: Border.all(color: Colors.yellow),
              ),
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
          ),
        ],
      ),
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

class _Help extends HookWidget {
  const _Help();

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
          width: double.infinity,
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

class _DeleteAccount extends StatelessWidget {
  const _DeleteAccount();

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionHeader(text: loc.userSettingsScreen_accountSection),
        const SizedBox(height: Spacings.xs),
        _FieldContainer(
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

class _FormLabel extends StatelessWidget {
  const _FormLabel(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
      child: Text(
        text,
        // Phone/Label/XS
        // Color/Text/Quaternary
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
        // Phone/Label/Regular Bold
        // Color/Text/Secondary
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
  const _FieldContainer({
    required this.child,
    this.width,
    this.height = 42,
    this.onTap,
  });

  final Widget child;
  final double? width;
  final double? height;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return DefaultTextStyle(
      style: TextStyle(
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
          width: width,
          height: height,
          child: child,
        ),
      ),
    );
  }
}

const outlineInputBorder = OutlineInputBorder(
  borderRadius: BorderRadius.all(Radius.circular(Spacings.s)),
  borderSide: BorderSide(width: 0, style: BorderStyle.none),
);

const inputDecoration = InputDecoration(
  contentPadding: EdgeInsets.symmetric(
    horizontal: Spacings.xxs,
    vertical: Spacings.xxs,
  ),
  isDense: true,
  border: outlineInputBorder,
  enabledBorder: outlineInputBorder,
  focusedBorder: outlineInputBorder,
);

const inputDecorationTheme = InputDecorationTheme(
  contentPadding: EdgeInsets.symmetric(
    horizontal: Spacings.xxs,
    vertical: Spacings.xxs,
  ),
  isDense: true,
  border: outlineInputBorder,
  enabledBorder: outlineInputBorder,
  focusedBorder: outlineInputBorder,
);

class _RemoveUsernameDialog extends StatelessWidget {
  const _RemoveUsernameDialog({required this.username});

  final UiUserHandle username;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return Dialog(
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Spacings.m),
      ),
      child: Container(
        constraints: const BoxConstraints(maxWidth: 340),
        padding: const EdgeInsets.only(
          left: Spacings.s,
          right: Spacings.s,
          top: Spacings.m,
          bottom: Spacings.s,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Text(
                loc.removeUsernameDialog_title,
                style: TextStyle(
                  fontSize: HeaderFontSize.h4.size,
                  fontWeight: FontWeight.bold,
                ),
              ),
            ),
            const SizedBox(height: Spacings.xxs),
            Text(
              loc.removeUsernameDialog_content,
              style: TextStyle(
                color: colors.text.secondary,
                fontSize: BodyFontSize.base.size,
              ),
            ),
            const SizedBox(height: Spacings.m),
            Row(
              children: [
                Expanded(
                  child: TextButton(
                    onPressed: () {
                      Navigator.of(context).pop(false);
                    },
                    style: _dialogButtonStyle.copyWith(
                      backgroundColor: WidgetStatePropertyAll(
                        colors.accent.quaternary,
                      ),
                    ),
                    child: Text(
                      loc.removeUsernameDialog_cancel,
                      style: TextStyle(fontSize: LabelFontSize.base.size),
                    ),
                  ),
                ),
                const SizedBox(width: Spacings.xs),
                Expanded(
                  child: TextButton(
                    onPressed: () {
                      context.read<UserCubit>().removeUserHandle(username);
                      Navigator.of(context).pop(true);
                    },
                    style: _dialogButtonStyle.copyWith(
                      backgroundColor: WidgetStatePropertyAll(
                        colors.function.danger,
                      ),
                      foregroundColor: WidgetStatePropertyAll(
                        colors.function.white,
                      ),
                    ),
                    child: Text(loc.removeUsernameDialog_remove),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _AddUsernameDialog extends HookWidget {
  const _AddUsernameDialog();

  @override
  Widget build(BuildContext context) {
    final formKey = useMemoized(() => GlobalKey<FormState>());

    final userHandleExists = useState(false);
    final isSubmitting = useState(false);

    final controller = useTextEditingController();
    final focusNode = useFocusNode();

    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return Dialog(
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Spacings.m),
      ),
      child: Container(
        constraints: const BoxConstraints(maxWidth: 340),
        padding: const EdgeInsets.only(
          left: Spacings.s,
          right: Spacings.s,
          top: Spacings.m,
          bottom: Spacings.s,
        ),
        child: Form(
          key: formKey,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Center(
                child: Text(
                  loc.userHandleScreen_title,
                  style: TextStyle(
                    fontSize: HeaderFontSize.h4.size,
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
              const SizedBox(height: Spacings.m),

              TextFormField(
                autocorrect: false,
                autofocus: true,
                controller: controller,
                focusNode: focusNode,
                inputFormatters: const [UserHandleInputFormatter()],
                validator: (value) => _validate(loc, userHandleExists, value),
                onChanged: (_) {
                  if (userHandleExists.value) {
                    userHandleExists.value = false;
                    formKey.currentState!.validate();
                  }
                },
                decoration: inputDecoration.copyWith(
                  hintText: loc.userHandleScreen_inputHint,
                  filled: true,
                  fillColor: colors.backgroundBase.secondary,
                ),
                onFieldSubmitted: (_) {
                  focusNode.requestFocus();
                  _submit(
                    context,
                    formKey,
                    controller,
                    userHandleExists,
                    isSubmitting,
                  );
                },
              ),

              const SizedBox(height: Spacings.xs),

              _FormLabel(loc.userHandleScreen_description),

              const SizedBox(height: Spacings.m),

              Row(
                children: [
                  Expanded(
                    child: TextButton(
                      onPressed: () {
                        Navigator.of(context).pop(false);
                      },
                      style: _dialogButtonStyle.copyWith(
                        backgroundColor: WidgetStatePropertyAll(
                          colors.accent.quaternary,
                        ),
                      ),
                      child: Text(
                        loc.userHandleScreen_cancel,
                        style: TextStyle(fontSize: LabelFontSize.base.size),
                      ),
                    ),
                  ),
                  const SizedBox(width: Spacings.xs),
                  Expanded(
                    child: TextButton(
                      onPressed:
                          () => _submit(
                            context,
                            formKey,
                            controller,
                            userHandleExists,
                            isSubmitting,
                          ),
                      style: _dialogButtonStyle.copyWith(
                        backgroundColor: WidgetStatePropertyAll(
                          colors.accent.primary,
                        ),
                        foregroundColor: WidgetStatePropertyAll(
                          colors.function.toggleWhite,
                        ),
                      ),
                      child:
                          !isSubmitting.value
                              ? Text(loc.userHandleScreen_confirm)
                              : SizedBox(
                                height: 20,
                                width: 20,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                  valueColor: AlwaysStoppedAnimation<Color>(
                                    colors.function.toggleWhite,
                                  ),
                                  backgroundColor: Colors.transparent,
                                ),
                              ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  void _submit(
    BuildContext context,
    GlobalKey<FormState> formKey,
    TextEditingController controller,
    ValueNotifier<bool> alreadyExists,
    ValueNotifier<bool> isSubmitting,
  ) async {
    if (!formKey.currentState!.validate()) {
      return;
    }
    final normalized = UserHandleInputFormatter.normalize(controller.text);
    final handle = UiUserHandle(plaintext: normalized);
    final userCubit = context.read<UserCubit>();

    // Clear already exists if any
    if (alreadyExists.value) {
      alreadyExists.value = false;
      formKey.currentState!.validate();
    }

    isSubmitting.value = true;
    if (!await userCubit.addUserHandle(handle)) {
      alreadyExists.value = true;
      isSubmitting.value = false;
      formKey.currentState!.validate();
      return;
    }
    if (!context.mounted) return;
    Navigator.of(context).pop();
  }

  String? _validate(
    AppLocalizations loc,
    ValueNotifier<bool> userHandleExists,
    String? value,
  ) {
    if (userHandleExists.value) {
      return loc.userHandleScreen_error_alreadyExists;
    }
    if (value == null || value.trim().isEmpty) {
      return loc.userHandleScreen_error_emptyHandle;
    }
    final safeValue = value;
    final normalized = UserHandleInputFormatter.normalize(safeValue);
    if (normalized.isEmpty) {
      return loc.userHandleScreen_error_emptyHandle;
    }
    final handle = UiUserHandle(plaintext: normalized);
    return handle.validationError();
  }
}

class _ChangeUsernameDialog extends HookWidget {
  const _ChangeUsernameDialog({required this.displayName});

  final String displayName;

  @override
  Widget build(BuildContext context) {
    final controller = useTextEditingController();
    useEffect(() {
      controller.text = displayName;
      return null;
    }, [displayName]);

    final focusNode = useFocusNode();

    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return Dialog(
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Spacings.m),
      ),
      child: Container(
        constraints: const BoxConstraints(maxWidth: 340),
        padding: const EdgeInsets.only(
          left: Spacings.s,
          right: Spacings.s,
          top: Spacings.m,
          bottom: Spacings.s,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Text(
                loc.editDisplayNameScreen_title,
                style: TextStyle(
                  fontSize: HeaderFontSize.h4.size,
                  fontWeight: FontWeight.bold,
                ),
              ),
            ),
            const SizedBox(height: Spacings.m),

            TextFormField(
              autocorrect: false,
              autofocus: true,
              controller: controller,
              focusNode: focusNode,
              decoration: inputDecoration.copyWith(
                filled: true,
                fillColor: colors.backgroundBase.secondary,
              ),
              onFieldSubmitted: (_) {
                focusNode.requestFocus();
                _submit(context, controller.text);
              },
            ),

            const SizedBox(height: Spacings.xs),

            _FormLabel(loc.editDisplayNameScreen_description),

            const SizedBox(height: Spacings.m),

            Row(
              children: [
                Expanded(
                  child: TextButton(
                    onPressed: () {
                      Navigator.of(context).pop(false);
                    },
                    style: _dialogButtonStyle.copyWith(
                      backgroundColor: WidgetStatePropertyAll(
                        colors.accent.quaternary,
                      ),
                    ),
                    child: Text(
                      loc.editDisplayNameScreen_cancel,
                      style: TextStyle(fontSize: LabelFontSize.base.size),
                    ),
                  ),
                ),
                const SizedBox(width: Spacings.xs),
                Expanded(
                  child: TextButton(
                    onPressed: () => _submit(context, controller.text),
                    style: _dialogButtonStyle.copyWith(
                      backgroundColor: WidgetStatePropertyAll(
                        colors.accent.primary,
                      ),
                      foregroundColor: WidgetStatePropertyAll(
                        colors.function.toggleWhite,
                      ),
                    ),
                    child: Text(loc.editDisplayNameScreen_save),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  void _submit(BuildContext context, String text) async {
    final userCubit = context.read<UserCubit>();
    userCubit.setProfile(displayName: text.trim());
    Navigator.of(context).pop();
  }
}

const _dialogButtonStyle = ButtonStyle(
  visualDensity: VisualDensity.compact,
  padding: WidgetStatePropertyAll(EdgeInsets.all(Spacings.sm)),
  shape: WidgetStatePropertyAll(
    RoundedRectangleBorder(
      borderRadius: BorderRadius.all(Radius.circular(Spacings.xs)),
    ),
  ),
);
