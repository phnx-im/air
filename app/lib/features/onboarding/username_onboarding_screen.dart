// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/constrained_width/constrained_width.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/util/username_input_formatter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

class UsernameOnboardingScreen extends HookWidget {
  const UsernameOnboardingScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final Color backgroundColor = palette.backgroundBase.secondary;
    final registrationState = context.watch<RegistrationCubit>().state;
    final initialHandle = UsernameInputFormatter.normalize(
      registrationState.usernameSuggestion ?? '',
    );

    final controller = useTextEditingController(text: initialHandle);
    final focusNode = useFocusNode();
    final usernameExists = useState(false);
    final usernameError = useState<String?>(null);
    final isSubmitting = useState(false);

    Future<void> submit() async {
      if (isSubmitting.value) {
        return;
      }
      usernameError.value = _validateUsername(
        loc,
        usernameExists.value,
        controller.text,
      );
      if (usernameError.value != null) {
        return;
      }
      final normalized = UsernameInputFormatter.normalize(
        controller.text.trim(),
      );
      final username = UiUsername(plaintext: normalized);
      final userCubit = context.read<UserCubit>();
      final navigationCubit = context.read<NavigationCubit>();
      final registrationCubit = context.read<RegistrationCubit>();
      usernameExists.value = false;
      isSubmitting.value = true;

      Future<bool> tryToAddUsername(bool displayFailure) async {
        try {
          final success = await userCubit.addUsername(username);
          if (success) {
            registrationCubit.clearUsernameOnboarding();
            navigationCubit.openHome();
          } else {
            usernameExists.value = true;
            isSubmitting.value = false;
            usernameError.value = _validateUsername(loc, true, controller.text);
          }
          return true;
        } catch (e) {
          if (displayFailure) {
            usernameExists.value = false;
            isSubmitting.value = false;
            showSnackBarStandalone(
              (loc) => SnackBar(content: Text(loc.usernameOnboarding_error)),
              tone: SnackbarTone.danger,
            );
          }
          return false;
        }
      }

      if (!await tryToAddUsername(false)) {
        // the privacy pass tokens for adding usernames might not yet be
        // available during account creation.
        await Future.delayed(const Duration(milliseconds: 250));
        tryToAddUsername(true);
      }
    }

    // A taken username is only known to be taken for the text that was sent,
    // so the next edit reopens the field to the rules it can still check.
    void onUsernameChanged(String value) {
      if (usernameExists.value) {
        usernameExists.value = false;
        usernameError.value = _validateUsername(loc, false, value);
      }
    }

    void skip() {
      if (isSubmitting.value) {
        return;
      }
      final registrationCubit = context.read<RegistrationCubit>();
      registrationCubit.clearUsernameOnboarding();
      context.read<NavigationCubit>().openHome();
    }

    return Scaffold(
      backgroundColor: backgroundColor,
      appBar: AppBar(
        automaticallyImplyLeading: false,
        title: Text(
          loc.usernameOnboarding_header,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        backgroundColor: backgroundColor,
        actionsPadding: const EdgeInsets.symmetric(horizontal: S.s16),
        actions: [
          Button(
            onPressed: skip,
            label: loc.usernameOnboarding_next,
            type: ButtonType.secondary,
            size: ButtonSize.small,
            state: isSubmitting.value
                ? ButtonState.disabled
                : ButtonState.active,
          ),
        ],
      ),
      body: SafeArea(
        child: Center(
          child: ConstrainedWidth(
            child: Column(
              children: [
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      return SingleChildScrollView(
                        padding: const EdgeInsets.symmetric(
                          horizontal: S.s16,
                          vertical: S.s12,
                        ),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              loc.usernameOnboarding_body,
                              textAlign: TextAlign.left,
                              style: Theme.of(context).textTheme.bodyMedium,
                            ),
                            const SizedBox(height: S.s24),
                            _UsernameTextField(
                              controller: controller,
                              focusNode: focusNode,
                              errorText: usernameError.value,
                              onChanged: onUsernameChanged,
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ),
                _AddButton(isSubmitting: isSubmitting.value, onPressed: submit),
                const SizedBox(height: S.s16),
              ],
            ),
          ),
        ),
      ),
    );
  }

  String? _validateUsername(
    AppLocalizations loc,
    bool alreadyExists,
    String value,
  ) {
    if (alreadyExists) {
      return loc.usernameScreen_error_alreadyExists;
    }
    if (value.trim().isEmpty) {
      return loc.usernameScreen_error_emptyUsername;
    }
    final normalized = UsernameInputFormatter.normalize(value);
    if (normalized.isEmpty) {
      return loc.usernameScreen_error_emptyUsername;
    }
    final username = UiUsername(plaintext: normalized);
    return switch (username.validationError()) {
      UsernameValidationError.tooShort => loc.usernameScreen_error_tooShort,
      UsernameValidationError.tooLong => loc.usernameScreen_error_tooLong,
      UsernameValidationError.invalidCharacter =>
        loc.usernameScreen_error_invalidCharacter,
      UsernameValidationError.consecutiveDashes =>
        loc.usernameScreen_error_consecutiveDashes,
      UsernameValidationError.leadingDigit =>
        loc.usernameScreen_error_leadingDigit,
      null => null,
    };
  }
}

class _AddButton extends StatelessWidget {
  const _AddButton({required this.isSubmitting, required this.onPressed});

  final bool isSubmitting;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: S.s24),
      width: context.breakpoint.isSmall ? double.infinity : null,
      child: Button(
        onPressed: onPressed,
        label: loc.usernameOnboarding_addButton,
        state: isSubmitting ? ButtonState.pending : ButtonState.active,
      ),
    );
  }
}

class _UsernameTextField extends StatelessWidget {
  const _UsernameTextField({
    required this.controller,
    required this.focusNode,
    required this.errorText,
    required this.onChanged,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final String? errorText;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    return AppTextInput(
      tokens: AppTextInputTokens.current,
      controller: controller,
      focusNode: focusNode,
      autofocus: true,
      textInputAction: TextInputAction.done,
      label: loc.usernameOnboarding_usernameInputName,
      hintText: loc.usernameOnboarding_usernameInputHint,
      helperText: loc.usernameOnboarding_syntax,
      errorText: errorText,
      inputFormatters: const [UsernameInputFormatter()],
      onChanged: onChanged,
    );
  }
}
