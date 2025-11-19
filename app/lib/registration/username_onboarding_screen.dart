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
import 'package:air/ui/theme/font.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/user_handle_input_formatter.dart';
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
    final initialHandle = UserHandleInputFormatter.normalize(
      registrationState.usernameSuggestion ?? '',
    );

    final formKey = useMemoized(() => GlobalKey<FormState>());
    final controller = useTextEditingController(text: initialHandle);
    final focusNode = useFocusNode();
    final handleExists = useState(false);
    final isSubmitting = useState(false);

    Future<void> submit() async {
      if (isSubmitting.value) {
        return;
      }
      if (!formKey.currentState!.validate()) {
        return;
      }
      final normalized = UserHandleInputFormatter.normalize(controller.text);
      final handle = UiUserHandle(plaintext: normalized);
      final userCubit = context.read<UserCubit>();
      final navigationCubit = context.read<NavigationCubit>();
      final registrationCubit = context.read<RegistrationCubit>();
      handleExists.value = false;
      isSubmitting.value = true;
      final success = await userCubit.addUserHandle(handle);
      if (!success) {
        handleExists.value = true;
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
      appBar: AppBar(
        automaticallyImplyLeading: false,
        title: Text(
          loc.usernameOnboarding_header,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        backgroundColor: backgroundColor,
        actions: [
          TextButton(
            onPressed: isSubmitting.value ? null : skip,
            child: Text(loc.usernameOnboarding_skip),
          ),
        ],
      ),
      body: Container(
        color: backgroundColor,
        child: SafeArea(
          child: ConstrainedWidth(
            child: Column(
              children: [
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      return SingleChildScrollView(
                        padding: const EdgeInsets.symmetric(
                          horizontal: Spacings.m,
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
                                handleExists: handleExists,
                                formKey: formKey,
                                onSubmitted: submit,
                                validator: (value) => _validateHandle(
                                  loc,
                                  handleExists.value,
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
              ],
            ),
          ),
        ),
      ),
    );
  }

  String? _validateHandle(
    AppLocalizations loc,
    bool alreadyExists,
    String? value,
  ) {
    if (alreadyExists) {
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
        style: OutlinedButton.styleFrom(
          textStyle: customTextScheme.labelMedium,
          backgroundColor: colors.accent.primary,
          foregroundColor: colors.function.toggleWhite,
        ),
        onPressed: isSubmitting ? null : onPressed,
        child: isSubmitting
            ? SizedBox(
                height: 20,
                width: 20,
                child: CircularProgressIndicator(
                  strokeWidth: 2,
                  valueColor: AlwaysStoppedAnimation<Color>(
                    colors.text.primary,
                  ),
                ),
              )
            : Text(loc.usernameOnboarding_addButton),
      ),
    );
  }
}

class _UsernameTextField extends StatelessWidget {
  const _UsernameTextField({
    required this.controller,
    required this.focusNode,
    required this.handleExists,
    required this.formKey,
    required this.onSubmitted,
    required this.validator,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final ValueNotifier<bool> handleExists;
  final GlobalKey<FormState> formKey;
  final VoidCallback onSubmitted;
  final FormFieldValidator<String>? validator;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: Spacings.xxs,
      children: [
        Text(
          loc.signUpScreen_displayNameInputName,
          style: TextStyle(
            fontSize: LabelFontSize.small2.size,
            color: colors.text.quaternary,
          ),
        ),
        TextFormField(
          autofocus: true,
          controller: controller,
          focusNode: focusNode,
          textInputAction: TextInputAction.done,
          decoration: InputDecoration(
            hintText: loc.userHandleScreen_inputHint,
            fillColor: colors.backgroundBase.tertiary,
          ),
          // Temporary strict enforcement until legacy underscores are fully removed.
          inputFormatters: const [UserHandleInputFormatter()],
          onChanged: (_) {
            if (handleExists.value) {
              handleExists.value = false;
              formKey.currentState?.validate();
            }
          },
          onFieldSubmitted: (_) => onSubmitted(),
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
