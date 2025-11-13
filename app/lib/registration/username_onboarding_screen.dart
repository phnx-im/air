// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/widgets/app_bar_button.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/registration/registration.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
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
        title: Text(loc.usernameOnboarding_title),
        actions: [
          AppBarButton(
            onPressed: isSubmitting.value ? null : skip,
            child: Text(
              loc.usernameOnboarding_skip,
              style: TextStyle(
                color: CustomColorScheme.of(context).function.danger,
              ),
            ),
          ),
        ],
      ),
      body: SafeArea(
        child: LayoutBuilder(
          builder: (context, constraints) {
            return SingleChildScrollView(
              padding: const EdgeInsets.symmetric(
                horizontal: Spacings.m,
                vertical: Spacings.xs,
              ),
              child: ConstrainedBox(
                constraints: BoxConstraints(minHeight: constraints.maxHeight),
                child: Form(
                  key: formKey,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        loc.usernameOnboarding_header,
                        style: Theme.of(context).textTheme.headlineMedium
                            ?.copyWith(fontWeight: FontWeight.bold),
                        textAlign: TextAlign.center,
                      ),
                      const SizedBox(height: Spacings.s),
                      Text(
                        loc.usernameOnboarding_body,
                        textAlign: TextAlign.left,
                        style: Theme.of(context).textTheme.bodyMedium,
                      ),
                      const SizedBox(height: Spacings.m),
                      TextFormField(
                        autofocus: true,
                        controller: controller,
                        focusNode: focusNode,
                        textInputAction: TextInputAction.done,
                        decoration: InputDecoration(
                          hintText: loc.userHandleScreen_inputHint,
                        ),
                        // Temporary strict enforcement until legacy underscores are fully removed.
                        inputFormatters: const [
                          UserHandleInputFormatter(),
                        ],
                        onChanged: (_) {
                          if (handleExists.value) {
                            handleExists.value = false;
                            formKey.currentState?.validate();
                          }
                        },
                        onFieldSubmitted: (_) => submit(),
                        validator:
                            (value) =>
                                _validateHandle(loc, handleExists.value, value),
                      ),
                      const SizedBox(height: Spacings.xs),
                      Text(
                        loc.usernameOnboarding_syntax,
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: CustomColorScheme.of(context).text.tertiary,
                        ),
                      ),
                      const SizedBox(height: Spacings.m),
                      OutlinedButton(
                        onPressed: isSubmitting.value ? null : submit,
                        child:
                            isSubmitting.value
                                ? SizedBox(
                                  height: 20,
                                  width: 20,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                    valueColor: AlwaysStoppedAnimation<Color>(
                                      CustomColorScheme.of(
                                        context,
                                      ).text.primary,
                                    ),
                                  ),
                                )
                                : Text(loc.usernameOnboarding_addButton),
                      ),
                      const SizedBox(height: Spacings.l),
                    ],
                  ),
                ),
              ),
            );
          },
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
