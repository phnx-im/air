// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:provider/provider.dart';

import 'registration_cubit.dart';

class SignUpScreen extends HookWidget {
  const SignUpScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final formKey = useMemoized(() => GlobalKey<FormState>());
    final showErrors = useState(false);

    final isKeyboardShown = MediaQuery.viewInsetsOf(context).bottom > 0;

    return Scaffold(
      resizeToAvoidBottomInset: true,
      appBar: AppBar(
        automaticallyImplyLeading: false,
        leading: const AppBarBackButton(),
        title: Text(loc.signUpScreen_header),
        toolbarHeight: isPointer() ? 100 : null,
      ),
      body: SafeArea(
        minimum: EdgeInsets.only(
          bottom: isKeyboardShown ? Spacings.s : Spacings.l + Spacings.xxs,
        ),
        child: Column(
          children: [
            Expanded(
              child: LayoutBuilder(
                builder: (context, constraints) {
                  return SingleChildScrollView(
                    keyboardDismissBehavior:
                        ScrollViewKeyboardDismissBehavior.onDrag,
                    padding: const EdgeInsets.only(
                      left: Spacings.s,
                      right: Spacings.s,
                      bottom: Spacings.xl,
                    ),
                    child: ConstrainedBox(
                      constraints: BoxConstraints(
                        minHeight: constraints.maxHeight,
                      ),
                      child: Align(
                        alignment: Alignment.topCenter,
                        child: _Form(
                          formKey: formKey,
                          showErrors: showErrors.value,
                        ),
                      ),
                    ),
                  );
                },
              ),
            ),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: Spacings.m),
              width: isSmallScreen(context) ? double.infinity : null,
              child: _SignUpButton(formKey: formKey, showErrors: showErrors),
            ),
          ],
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
          ? const Size(double.infinity, 80)
          : const Size(300, 80),
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
            const SizedBox(height: Spacings.xs),
            Text(
              loc.signUpScreen_subheader,
              style: Theme.of(context).textTheme.bodyMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: Spacings.m),

            GestureDetector(
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
              const SizedBox(height: Spacings.m),

              Text(loc.signUpScreen_serverLabel),
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

class _UserAvatarPicker extends StatelessWidget {
  const _UserAvatarPicker();

  @override
  Widget build(BuildContext context) {
    final (displayName, avatar) = context.select(
      (RegistrationCubit cubit) =>
          (cubit.state.displayName, cubit.state.avatar),
    );

    final colors = CustomColorScheme.of(context);
    final showPlaceholderIcon = avatar == null;

    return SizedBox(
      width: 192,
      height: 192,
      child: Stack(
        alignment: Alignment.center,
        children: [
          UserAvatar(
            displayName: displayName,
            image: avatar,
            size: 192,
            onPressed: () async {
              var registrationCubit = context.read<RegistrationCubit>();
              final ImagePicker picker = ImagePicker();
              final XFile? image = await picker.pickImage(
                source: ImageSource.gallery,
              );
              final bytes = await image?.readAsBytes();
              registrationCubit.setAvatar(bytes?.toImageData());
            },
          ),
          if (showPlaceholderIcon)
            IgnorePointer(
              child: IconTheme(
                data: const IconThemeData(),
                child: iconoir.MediaImagePlus(
                  width: 24,
                  color: colors.text.primary,
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

    final focusNode = useFocusNode();

    return TextFormField(
      autofocus: isSmallScreen(context) ? false : true,
      decoration: InputDecoration(hintText: loc.signUpScreen_displayNameHint),
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
    );
  }
}

class _ServerTextField extends HookWidget {
  const _ServerTextField({required this.onFieldSubmitted});

  final VoidCallback onFieldSubmitted;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final focusNode = useFocusNode();

    return TextFormField(
      decoration: InputDecoration(hintText: loc.signUpScreen_serverHint),
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
    final isSigningUp = context.select(
      (RegistrationCubit cubit) => cubit.state.isSigningUp,
    );
    return OutlinedButton(
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
          ? const CircularProgressIndicator()
          : Text(loc.signUpScreen_actionButton),
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
    navigationCubit.pop();
    navigationCubit.openIntroScreen(const IntroScreenType.usernameOnboarding());
  } else if (context.mounted) {
    final loc = AppLocalizations.of(context);
    showErrorBanner(context, loc.signUpScreen_error_register(error.message));
  }
}
