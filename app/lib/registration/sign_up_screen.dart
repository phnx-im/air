// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/desktop/width_constraints.dart';
import 'package:air/ui/icons/app_icon.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/widgets/widgets.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:provider/provider.dart';

import 'registration_cubit.dart';

class SignUpScreen extends HookWidget {
  const SignUpScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    final backgroundColor = colors.backgroundBase.secondary;

    final formKey = useMemoized(() => GlobalKey<FormState>());
    final showErrors = useState(false);

    return Scaffold(
      resizeToAvoidBottomInset: true,
      appBar: AppBar(
        automaticallyImplyLeading: false,
        leading: AppBarBackButton(
          backgroundColor: colors.backgroundElevated.primary,
        ),
        title: Text(
          loc.signUpScreen_header,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        toolbarHeight: isPointer() ? 100 : null,
        backgroundColor: Colors.transparent,
      ),
      backgroundColor: backgroundColor,
      body: SafeArea(
        child: Center(
          child: ConstrainedWidth(
            child: Column(
              children: [
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      return SingleChildScrollView(
                        keyboardDismissBehavior:
                            ScrollViewKeyboardDismissBehavior.onDrag,
                        padding: const EdgeInsets.symmetric(
                          horizontal: Spacings.s,
                          vertical: Spacings.xs,
                        ),
                        child: _Form(
                          formKey: formKey,
                          showErrors: showErrors.value,
                        ),
                      );
                    },
                  ),
                ),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: Spacings.m),
                  width: isSmallScreen(context) ? double.infinity : null,
                  child: _SignUpButton(
                    formKey: formKey,
                    showErrors: showErrors,
                  ),
                ),
                const SizedBox(height: Spacings.s),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Form extends HookWidget {
  const _Form({required this.formKey, required this.showErrors});

  final GlobalKey<FormState> formKey;
  final bool showErrors;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final textFormContstraints = BoxConstraints.tight(
      isSmallScreen(context)
          ? const Size(double.infinity, 120)
          : const Size(300, 120),
    );

    final serverFieldVisible = useState(false);

    return Form(
      key: formKey,
      autovalidateMode: showErrors
          ? AutovalidateMode.always
          : AutovalidateMode.disabled,
      child: Center(
        child: Column(
          children: [
            Text(
              loc.signUpScreen_subheader,
              style: Theme.of(context).textTheme.bodyMedium,
              textAlign: TextAlign.left,
            ),
            const SizedBox(height: Spacings.l),

            GestureDetector(
              onTap: () => _pickAvatar(context),
              onLongPress: () => serverFieldVisible.value = true,
              child: const _UserAvatarPicker(),
            ),
            const SizedBox(height: Spacings.l),

            ConstrainedBox(
              constraints: textFormContstraints,
              child: _DisplayNameTextField(
                onFieldSubmitted: () => _submit(context, formKey),
              ),
            ),

            if (serverFieldVisible.value) ...[
              Text(
                loc.signUpScreen_serverLabel,
                style: Theme.of(context).textTheme.bodyMedium,
                textAlign: TextAlign.left,
              ),
              const SizedBox(height: Spacings.s),

              ConstrainedBox(
                constraints: textFormContstraints,
                child: _ServerTextField(
                  onFieldSubmitted: () => _submit(context, formKey),
                ),
              ),
            ],

            const SizedBox(height: Spacings.s),
          ],
        ),
      ),
    );
  }
}

Future<void> _pickAvatar(BuildContext context) async {
  final registrationCubit = context.read<RegistrationCubit>();
  final ImagePicker picker = ImagePicker();
  final XFile? image = await picker.pickImage(source: ImageSource.gallery);
  final bytes = await image?.readAsBytes();
  registrationCubit.setAvatar(bytes?.toImageData());
}

class _UserAvatarPicker extends StatelessWidget {
  const _UserAvatarPicker();

  static const double size = 96;

  @override
  Widget build(BuildContext context) {
    final avatar = context.select(
      (RegistrationCubit cubit) => cubit.state.avatar,
    );

    final colors = CustomColorScheme.of(context);
    final showPlaceholderIcon = avatar == null;

    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        alignment: Alignment.center,
        children: [
          if (!showPlaceholderIcon)
            ClipOval(
              child: Image(
                width: size,
                height: size,
                fit: BoxFit.cover,
                image: CachedMemoryImage.fromImageData(avatar),
              ),
            ),
          // Circle overlay with icon
          if (showPlaceholderIcon)
            Container(
              width: size,
              height: size,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: colors.fill.tertiary,
              ),
              alignment: Alignment.center,
              child: IgnorePointer(
                child: IconTheme(
                  data: const IconThemeData(),
                  child: AppIcon(
                    type: AppIconType.mediaImagePlus,
                    size: 24,
                    color: colors.text.primary,
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _DisplayNameTextField extends HookWidget {
  const _DisplayNameTextField({required this.onFieldSubmitted});

  final VoidCallback onFieldSubmitted;

  @override
  Widget build(BuildContext context) {
    final displayName = context.read<RegistrationCubit>().state.displayName;

    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final focusNode = useFocusNode();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: Spacings.xxs,
      children: [
        Padding(
          padding: const EdgeInsets.only(left: Spacings.xxs),
          child: Text(
            loc.signUpScreen_displayNameInputName,
            style: TextStyle(
              fontSize: LabelFontSize.small2.size,
              color: colors.text.quaternary,
            ),
          ),
        ),
        TextFormField(
          autofocus: true,
          decoration: InputDecoration(
            hintText: loc.signUpScreen_displayNameInputHint,
            fillColor: colors.backgroundBase.tertiary,
          ),
          initialValue: displayName,
          onChanged: (value) {
            context.read<RegistrationCubit>().setDisplayName(value);
          },
          onFieldSubmitted: (_) {
            focusNode.requestFocus();
            onFieldSubmitted();
          },
          validator: (value) =>
              context.read<RegistrationCubit>().state.displayName.trim().isEmpty
              ? loc.signUpScreen_error_emptyDisplayName
              : null,
        ),
      ],
    );
  }
}

class _ServerTextField extends HookWidget {
  const _ServerTextField({required this.onFieldSubmitted});

  final VoidCallback onFieldSubmitted;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final colors = CustomColorScheme.of(context);

    final focusNode = useFocusNode();

    return TextFormField(
      decoration: InputDecoration(
        hintText: loc.signUpScreen_serverHint,
        fillColor: colors.backgroundBase.tertiary,
      ),
      initialValue: context.read<RegistrationCubit>().state.domain,
      focusNode: focusNode,
      onChanged: (String value) {
        context.read<RegistrationCubit>().setDomain(value);
      },
      onFieldSubmitted: (_) {
        focusNode.requestFocus();
        onFieldSubmitted();
      },
      validator: (value) =>
          context.read<RegistrationCubit>().state.isDomainValid
          ? null
          : loc.signUpScreen_error_invalidDomain,
    );
  }
}

class _SignUpButton extends StatelessWidget {
  const _SignUpButton({required this.formKey, required this.showErrors});

  final GlobalKey<FormState> formKey;
  final ValueNotifier<bool> showErrors;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    final isSigningUp = context.select(
      (RegistrationCubit cubit) => cubit.state.isSigningUp,
    );
    return OutlinedButton(
      style: OutlinedButtonTheme.of(context).style!.copyWith(
        backgroundColor: WidgetStateProperty.all(colors.accent.primary),
        foregroundColor: WidgetStateProperty.all(colors.function.toggleWhite),
      ),
      onPressed: isSigningUp
          ? null
          : () {
              showErrors.value = true;
              if (!formKey.currentState!.validate()) {
                return;
              }
              _submit(context, formKey);
            },
      child: isSigningUp
          ? SizedBox(
              height: 20,
              width: 20,
              child: CircularProgressIndicator(
                strokeWidth: 2,
                valueColor: AlwaysStoppedAnimation<Color>(
                  colors.function.toggleWhite,
                ),
              ),
            )
          : Text(
              loc.signUpScreen_actionButton,
              style: TextStyle(
                color: colors.function.toggleWhite,
                fontSize: LabelFontSize.base.size,
              ),
            ),
    );
  }
}

void _submit(BuildContext context, GlobalKey<FormState> formKey) async {
  if (!formKey.currentState!.validate()) {
    return;
  }

  final navigationCubit = context.read<NavigationCubit>();
  final registrationCubit = context.read<RegistrationCubit>();
  final error = await registrationCubit.signUp();
  if (error == null) {
    String suggestion = registrationCubit.state.usernameSuggestion ?? '';
    if (suggestion.isEmpty) {
      try {
        suggestion = usernameFromDisplay(
          display: registrationCubit.state.displayName,
        );
      } catch (_) {
        suggestion = registrationCubit.state.displayName.trim().toLowerCase();
      }
      if (suggestion.isEmpty) {
        suggestion = 'user';
      }
    }
    if (!context.mounted) {
      return;
    }
    registrationCubit.startUsernameOnboarding(suggestion);
    navigationCubit.pop(); // Invitation code screen
    navigationCubit.pop(); // Sign up screen
    navigationCubit.openIntroScreen(const IntroScreenType.usernameOnboarding());
  } else if (context.mounted) {
    final loc = AppLocalizations.of(context);
    showErrorBanner(context, loc.signUpScreen_error_register(error.message));
  }
}
