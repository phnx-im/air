// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/registration/registration.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/desktop/width_constraints.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/username_input_formatter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

class UsernameOnboardingScreen extends HookWidget {
  const UsernameOnboardingScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    final Color backgroundColor = colors.backgroundBase.secondary;
    final registrationState = context.watch<RegistrationCubit>().state;
    final initialHandle = UsernameInputFormatter.normalize(
      registrationState.usernameSuggestion ?? '',
    );

    final formKey = useMemoized(() => GlobalKey<FormState>());
    final controller = useTextEditingController(text: initialHandle);
    final focusNode = useFocusNode();
    final usernameExists = useState(false);
    final isSubmitting = useState(false);

    Future<void> submit() async {
      if (isSubmitting.value) {
        return;
      }
      if (!formKey.currentState!.validate()) {
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
      final success = await userCubit.addUsername(username);
      if (!success) {
        usernameExists.value = true;
        isSubmitting.value = false;
        formKey.currentState!.validate();
        return;
      }
      registrationCubit.clearUsernameOnboarding();
      navigationCubit.openHome();
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
        actionsPadding: const EdgeInsets.symmetric(horizontal: Spacings.s),
        actions: [
          TextButton(
            onPressed: isSubmitting.value ? null : skip,
            child: Text(loc.usernameOnboarding_next),
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
                          horizontal: Spacings.s,
                          vertical: Spacings.xs,
                        ),
                        child: Form(
                          key: formKey,
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                loc.usernameOnboarding_body,
                                textAlign: TextAlign.left,
                                style: Theme.of(context).textTheme.bodyMedium,
                              ),
                              const SizedBox(height: Spacings.m),
                              _UsernameTextField(
                                controller: controller,
                                focusNode: focusNode,
                                usernameExists: usernameExists,
                                formKey: formKey,
                                validator: (value) => _validateUsername(
                                  loc,
                                  usernameExists.value,
                                  value,
                                ),
                              ),
                            ],
                          ),
                        ),
                      );
                    },
                  ),
                ),
                _AddButton(isSubmitting: isSubmitting.value, onPressed: submit),
                const SizedBox(height: Spacings.s),
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
    String? value,
  ) {
    if (alreadyExists) {
      return loc.usernameScreen_error_alreadyExists;
    }
    if (value == null || value.trim().isEmpty) {
      return loc.usernameScreen_error_emptyUsername;
    }
    final safeValue = value;
    final normalized = UsernameInputFormatter.normalize(safeValue);
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
    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: Spacings.m),
      width: isSmallScreen(context) ? double.infinity : null,
      child: OutlinedButton(
        style: OutlinedButtonTheme.of(context).style!.copyWith(
          backgroundColor: WidgetStateProperty.all(colors.accent.primary),
          foregroundColor: WidgetStateProperty.all(colors.function.toggleWhite),
        ),
        onPressed: isSubmitting ? null : onPressed,
        child: isSubmitting
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
                loc.usernameOnboarding_addButton,
                style: TextStyle(
                  color: colors.function.toggleWhite,
                  fontSize: LabelFontSize.base.size,
                ),
              ),
      ),
    );
  }
}

class _UsernameTextField extends StatelessWidget {
  const _UsernameTextField({
    required this.controller,
    required this.focusNode,
    required this.usernameExists,
    required this.formKey,
    required this.validator,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final ValueNotifier<bool> usernameExists;
  final GlobalKey<FormState> formKey;
  final FormFieldValidator<String>? validator;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: Spacings.xxs,
      children: [
        Padding(
          padding: const EdgeInsets.only(left: Spacings.xxs),
          child: Text(
            loc.usernameOnboarding_usernameInputName,
            style: TextStyle(
              fontSize: LabelFontSize.small2.size,
              color: colors.text.quaternary,
            ),
          ),
        ),
        TextFormField(
          autofocus: true,
          controller: controller,
          focusNode: focusNode,
          textInputAction: TextInputAction.done,
          decoration: InputDecoration(
            hintText: loc.usernameOnboarding_usernameInputHint,
            fillColor: colors.backgroundBase.tertiary,
          ),
          inputFormatters: const [UsernameInputFormatter()],
          onChanged: (_) {
            if (usernameExists.value) {
              usernameExists.value = false;
              formKey.currentState?.validate();
            }
          },
          validator: validator,
        ),
        Text(
          loc.usernameOnboarding_syntax,
          style: TextStyle(
            fontSize: LabelFontSize.small2.size,
            color: colors.text.quaternary,
          ),
        ),
      ],
    );
  }
}
