// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/constrained_width/constrained_width.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/features/navigation/app_bar_back_button.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';

import 'package:air/features/onboarding/registration_cubit.dart';

/// An invitation code is exactly this long, so the same number caps the field,
/// bounds the counter under it, and decides whether the code is complete.
const int _codeLength = 8;

class InvitationCodeScreen extends HookWidget {
  const InvitationCodeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);
    final backgroundColor = palette.backgroundBase.secondary;

    final serverFieldVisible = useState(false);
    final showErrors = useState(false);
    final codeError = useState<String?>(null);
    final domainError = useState<String?>(null);

    bool validate() {
      final state = context.read<RegistrationCubit>().state;
      final code = state.invitationCode;
      codeError.value = code == null || code.length != _codeLength
          ? loc.invitationCodeScreen_error_invalidLength
          : null;
      // The server field only carries a rule while it is on screen.
      domainError.value = serverFieldVisible.value && !state.isDomainValid
          ? loc.signUpScreen_error_invalidDomain
          : null;
      return codeError.value == null && domainError.value == null;
    }

    // Typing only moves the errors once the user has asked to join, so the
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
        clipBehavior: Clip.none,
        leading: AppBarBackButton(
          backgroundColor: palette.backgroundElevated.primary,
        ),
        title: Text(
          loc.invitationCodeScreen_header,
          style: const TextStyle(fontWeight: FontWeight.bold),
        ),
        backgroundColor: palette.backgroundBase.secondary,
      ),
      backgroundColor: backgroundColor,
      body: SafeArea(
        child: Center(
          child: ConstrainedWidth(
            child: Column(
              crossAxisAlignment: .center,
              children: [
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      return SingleChildScrollView(
                        padding: const EdgeInsets.symmetric(
                          horizontal: S.s16,
                          vertical: S.s12,
                        ),
                        child: _Body(
                          serverFieldVisible: serverFieldVisible.value,
                          onRevealServerField: () =>
                              serverFieldVisible.value = true,
                          codeError: codeError.value,
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
                  child: _JoinButton(
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
    required this.codeError,
    required this.domainError,
    required this.onChanged,
    required this.onSubmitted,
  });

  final bool serverFieldVisible;
  final VoidCallback onRevealServerField;
  final String? codeError;
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

    return Column(
      crossAxisAlignment: .center,
      children: [
        GestureDetector(
          onLongPress: onRevealServerField,
          child: Text(
            loc.invitationCodeScreen_subheader,
            style: Theme.of(context).textTheme.bodyMedium,
            textAlign: TextAlign.left,
          ),
        ),
        const SizedBox(height: S.s64),

        ConstrainedBox(
          constraints: textFormConstraints,
          child: _InvitationCodeTextField(
            errorText: codeError,
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
    );
  }
}

class _InvitationCodeTextField extends StatelessWidget {
  const _InvitationCodeTextField({
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
    final length = context.select(
      (RegistrationCubit cubit) => cubit.state.invitationCode?.length ?? 0,
    );

    const allowedCharactersRegex = r'[A-HJKMNP-Z0-9]';
    // The keyboard's shift state is only a hint, and a hardware keyboard
    // ignores it, so input is folded before the filter below drops whatever
    // falls outside the code alphabet.
    final upperCaseFormatter = TextInputFormatter.withFunction(
      (oldValue, newValue) =>
          newValue.copyWith(text: newValue.text.toUpperCase()),
    );
    final inputFormatter = FilteringTextInputFormatter.allow(
      RegExp(allowedCharactersRegex),
    );

    return AppTextInput(
      tokens: AppTextInputTokens.of(context),
      autofocus: true,
      label: loc.invitationCodeScreen_inputLabel,
      hintText: loc.invitationCodeScreen_inputHint,
      // The field caps the length silently, so how far along the code is has
      // to be said here. An error takes the same line, being the more urgent
      // of the two.
      helperText: '$length/$_codeLength',
      errorText: errorText,
      maxLength: _codeLength,
      inputFormatters: [upperCaseFormatter, inputFormatter],
      textCapitalization: TextCapitalization.characters,
      keyboardType: TextInputType.visiblePassword,
      onChanged: (value) {
        context.read<RegistrationCubit>().setInvitationCode(value);
        onChanged();
      },
      onSubmitted: (_) => onSubmitted(),
    );
  }
}

class _JoinButton extends StatelessWidget {
  const _JoinButton({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final isCheckingInvitationCode = context.select(
      (RegistrationCubit cubit) => cubit.state.isCheckingInvitationCode,
    );

    return Button(
      onPressed: onPressed,
      label: loc.invitationCodeScreen_actionButton,
      state: isCheckingInvitationCode
          ? ButtonState.pending
          : ButtonState.active,
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
      tokens: AppTextInputTokens.of(context),
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

void _submit(BuildContext context) async {
  final navigationCubit = context.read<NavigationCubit>();
  final registrationCubit = context.read<RegistrationCubit>();

  final error = await registrationCubit.submitInvitationCode();

  if (error == null) {
    navigationCubit.openIntroScreen(const IntroScreenType.signUp());
  } else {
    showErrorBannerStandalone(
      (loc) => switch (error.code) {
        .missing => loc.invitationCodeScreen_error_missing,
        .invalid => loc.invitationCodeScreen_error_invalid,
        .internal => loc.invitationCodeScreen_error_internal(
          error.message ?? "",
        ),
      },
    );
  }
}
