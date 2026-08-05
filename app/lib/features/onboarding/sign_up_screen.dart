// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/constrained_width/constrained_width.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:provider/provider.dart';

import 'package:air/features/onboarding/registration_cubit.dart';

class SignUpScreen extends HookWidget {
  const SignUpScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final backgroundColor = palette.backgroundBase.secondary;

    final serverFieldVisible = useState(false);
    final showErrors = useState(false);
    final displayNameError = useState<String?>(null);
    final domainError = useState<String?>(null);

    bool validate() {
      final state = context.read<RegistrationCubit>().state;
      displayNameError.value = state.displayName.trim().isEmpty
          ? loc.signUpScreen_error_emptyDisplayName
          : null;
      // The server field only carries a rule while it is on screen.
      domainError.value = serverFieldVisible.value && !state.isDomainValid
          ? loc.signUpScreen_error_invalidDomain
          : null;
      return displayNameError.value == null && domainError.value == null;
    }

    // Typing only moves the errors once the user has asked to sign up, so the
    // first attempt is what puts them on screen.
    void revalidate() {
      if (showErrors.value) {
        validate();
      }
    }

    void submit() {
      if (validate()) {
        _submit(context);
      }
    }

    return Scaffold(
      resizeToAvoidBottomInset: true,
      appBar: AppBar(
        automaticallyImplyLeading: false,
        clipBehavior: Clip.none,
        leading: AppBarBackButton(
          backgroundColor: palette.backgroundElevated.primary,
        ),
        title: Text(
          loc.signUpScreen_header,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        backgroundColor: palette.backgroundBase.secondary,
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
                          horizontal: S.s16,
                          vertical: S.s12,
                        ),
                        child: _Body(
                          serverFieldVisible: serverFieldVisible.value,
                          onRevealServerField: () =>
                              serverFieldVisible.value = true,
                          displayNameError: displayNameError.value,
                          domainError: domainError.value,
                          onChanged: revalidate,
                          onSubmitted: submit,
                        ),
                      );
                    },
                  ),
                ),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: S.s24),
                  width: context.breakpoint.isSmall ? double.infinity : null,
                  child: _SignUpButton(
                    onPressed: () {
                      showErrors.value = true;
                      submit();
                    },
                  ),
                ),
                const SizedBox(height: S.s16),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Body extends StatelessWidget {
  const _Body({
    required this.serverFieldVisible,
    required this.onRevealServerField,
    required this.displayNameError,
    required this.domainError,
    required this.onChanged,
    required this.onSubmitted,
  });

  final bool serverFieldVisible;
  final VoidCallback onRevealServerField;
  final String? displayNameError;
  final String? domainError;
  final VoidCallback onChanged;
  final VoidCallback onSubmitted;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final textFormConstraints = BoxConstraints.tight(
      context.breakpoint.isSmall
          ? const Size(double.infinity, 120)
          : const Size(300, 120),
    );

    return Center(
      child: Column(
        children: [
          Text(
            loc.signUpScreen_subheader,
            style: Theme.of(context).textTheme.bodyMedium,
            textAlign: TextAlign.left,
          ),
          const SizedBox(height: S.s32),

          GestureDetector(
            onTap: () => _pickAvatar(context),
            onLongPress: onRevealServerField,
            child: const _UserAvatarPicker(),
          ),
          const SizedBox(height: S.s32),

          ConstrainedBox(
            constraints: textFormConstraints,
            child: _DisplayNameTextField(
              errorText: displayNameError,
              onChanged: onChanged,
              onSubmitted: onSubmitted,
            ),
          ),

          if (serverFieldVisible) ...[
            Text(
              loc.signUpScreen_serverLabel,
              style: Theme.of(context).textTheme.bodyMedium,
              textAlign: TextAlign.left,
            ),
            const SizedBox(height: S.s16),

            ConstrainedBox(
              constraints: textFormConstraints,
              child: _ServerTextField(
                errorText: domainError,
                onChanged: onChanged,
                onSubmitted: onSubmitted,
              ),
            ),
          ],

          const SizedBox(height: S.s16),
        ],
      ),
    );
  }
}

Future<void> _pickAvatar(BuildContext context) async {
  final registrationCubit = context.read<RegistrationCubit>();
  final ImagePicker picker = ImagePicker();
  // Reduce image quality to re-encode the image.
  final XFile? image = await picker.pickImage(
    source: ImageSource.gallery,
    imageQuality: 99,
  );
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

    final palette = SemanticPalette.of(context);
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
                color: palette.fill.tertiary,
              ),
              alignment: Alignment.center,
              child: const IgnorePointer(
                child: IconTheme(
                  data: IconThemeData(),
                  child: AppIcon.imagePlus(size: 24),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _DisplayNameTextField extends HookWidget {
  const _DisplayNameTextField({
    required this.errorText,
    required this.onChanged,
    required this.onSubmitted,
  });

  final String? errorText;
  final VoidCallback onChanged;
  final VoidCallback onSubmitted;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final controller = useTextEditingController(
      text: context.read<RegistrationCubit>().state.displayName,
    );

    return AppTextInput(
      tokens: AppTextInputTokens.current,
      controller: controller,
      autofocus: true,
      label: loc.signUpScreen_displayNameInputName,
      hintText: loc.signUpScreen_displayNameInputHint,
      errorText: errorText,
      onChanged: (value) {
        context.read<RegistrationCubit>().setDisplayName(value);
        onChanged();
      },
      onSubmitted: (_) => onSubmitted(),
    );
  }
}

class _ServerTextField extends HookWidget {
  const _ServerTextField({
    required this.errorText,
    required this.onChanged,
    required this.onSubmitted,
  });

  final String? errorText;
  final VoidCallback onChanged;
  final VoidCallback onSubmitted;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final focusNode = useFocusNode();
    final controller = useTextEditingController(
      text: context.read<RegistrationCubit>().state.domain,
    );

    return AppTextInput(
      tokens: AppTextInputTokens.current,
      controller: controller,
      focusNode: focusNode,
      hintText: loc.signUpScreen_serverHint,
      errorText: errorText,
      onChanged: (String value) {
        context.read<RegistrationCubit>().setDomain(value);
        onChanged();
      },
      onSubmitted: (_) {
        focusNode.requestFocus();
        onSubmitted();
      },
    );
  }
}

class _SignUpButton extends StatelessWidget {
  const _SignUpButton({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final isSigningUp = context.select(
      (RegistrationCubit cubit) => cubit.state.isSigningUp,
    );
    return Button(
      onPressed: onPressed,
      label: loc.signUpScreen_actionButton,
      state: isSigningUp ? ButtonState.pending : ButtonState.active,
    );
  }
}

void _submit(BuildContext context) async {
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
  } else {
    showErrorBannerStandalone(
      (loc) => loc.signUpScreen_error_register(error.message),
    );
  }
}
