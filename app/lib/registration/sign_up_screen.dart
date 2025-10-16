// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:flutter/material.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:image_picker/image_picker.dart';
import 'package:provider/provider.dart';

import 'registration_cubit.dart';

class SignUpScreen extends HookWidget {
  const SignUpScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final formKey = useMemoized(() => GlobalKey<FormState>());

    final isKeyboardShown = MediaQuery.viewInsetsOf(context).bottom > 0;

    return Scaffold(
      resizeToAvoidBottomInset: true,
      appBar: AppBar(
        title: Text(loc.signUpScreen_title),
        toolbarHeight: isPointer() ? 100 : null,
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        minimum: EdgeInsets.only(
          bottom: isKeyboardShown ? Spacings.s : Spacings.l + Spacings.xxs,
        ),
        child: Stack(
          fit: StackFit.expand,
          children: [
            // SingleChildScrollView prevents the content resizing when virtual keyboard is shown
            SingleChildScrollView(
              physics: const NeverScrollableScrollPhysics(),
              keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
              child: Padding(
                padding: const EdgeInsets.only(
                  left: Spacings.s,
                  right: Spacings.s,
                ),
                child: _Form(formKey: formKey),
              ),
            ),
            Column(
              children: [
                const Spacer(),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: Spacings.m),
                  width: isSmallScreen(context) ? double.infinity : null,
                  child: _SignUpButton(formKey: formKey),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _Form extends HookWidget {
  const _Form({required this.formKey});

  final GlobalKey<FormState> formKey;

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
      autovalidateMode: AutovalidateMode.onUserInteraction,
      child: Center(
        child: Column(
          children: [
            const SizedBox(height: Spacings.s),

            GestureDetector(
              onLongPress: () => serverFieldVisible.value = true,
              child: const _UserAvatarPicker(),
            ),
            const SizedBox(height: Spacings.s),

            Text(loc.signUpScreen_displayNameLabel),
            const SizedBox(height: Spacings.s),

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
      (RegistrationCubit cubit) => (
        cubit.state.displayName,
        cubit.state.avatar,
      ),
    );

    return UserAvatar(
      displayName: displayName,
      image: avatar,
      size: 100,
      onPressed: () async {
        var registrationCubit = context.read<RegistrationCubit>();
        // Image picker
        final ImagePicker picker = ImagePicker();
        // Pick an image.
        final XFile? image = await picker.pickImage(
          source: ImageSource.gallery,
        );
        final bytes = await image?.readAsBytes();
        registrationCubit.setAvatar(bytes?.toImageData());
      },
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
      validator:
          (value) =>
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
      validator:
          (value) =>
              context.read<RegistrationCubit>().state.isDomainValid
                  ? null
                  : loc.signUpScreen_error_invalidDomain,
    );
  }
}

class _SignUpButton extends StatelessWidget {
  const _SignUpButton({required this.formKey});

  final GlobalKey<FormState> formKey;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final (isValid, isSigningUp) = context.select(
      (RegistrationCubit cubit) => (
        cubit.state.isValid,
        cubit.state.isSigningUp,
      ),
    );
    return OutlinedButton(
      onPressed:
          isValid && !isSigningUp ? () => _submit(context, formKey) : null,
      child:
          isSigningUp
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
  final error = await context.read<RegistrationCubit>().signUp();
  if (error == null) {
    navigationCubit.openHome();
  } else if (context.mounted) {
    final loc = AppLocalizations.of(context);
    showErrorBanner(context, loc.signUpScreen_error_register(error.message));
  }
}
