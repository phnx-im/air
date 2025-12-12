// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/desktop/width_constraints.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:flutter/services.dart'; // Import for FilteringTextInputFormatter
import 'package:provider/provider.dart';

import 'registration_cubit.dart';

class InvitationCodeScreen extends HookWidget {
  const InvitationCodeScreen({super.key});

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
        leading: AppBarBackButton(
          backgroundColor: colors.backgroundElevated.primary,
        ),
        title: Text(
          loc.invitationCodeScreen_header,
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
              crossAxisAlignment: .center,
              children: [
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      return SingleChildScrollView(
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
                  child: _JoinButton(formKey: formKey, showErrors: showErrors),
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
      autovalidateMode: showErrors ? .always : .disabled,
      child: Column(
        crossAxisAlignment: .center,
        children: [
          GestureDetector(
            onLongPress: () => serverFieldVisible.value = true,
            child: Text(
              loc.invitationCodeScreen_subheader,
              style: Theme.of(context).textTheme.bodyMedium,
              textAlign: TextAlign.left,
            ),
          ),
          const SizedBox(height: Spacings.xxl),

          ConstrainedBox(
            constraints: textFormContstraints,
            child: _InvitationCodeTextField(
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
    );
  }
}

class _InvitationCodeTextField extends HookWidget {
  const _InvitationCodeTextField({required this.onFieldSubmitted});

  final VoidCallback onFieldSubmitted;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final focusNode = useFocusNode();

    const allowedCharactersRegex = r'[A-HJKMNP-Z0-9]';
    final inputFormatter = FilteringTextInputFormatter.allow(
      RegExp(allowedCharactersRegex),
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      spacing: Spacings.xxs,
      children: [
        Padding(
          padding: const EdgeInsets.only(left: Spacings.xxs),
          child: Text(
            loc.invitationCodeScreen_inputLabel,
            style: TextStyle(
              fontSize: LabelFontSize.small2.size,
              color: colors.text.quaternary,
            ),
          ),
        ),
        TextFormField(
          autofocus: true,
          decoration: InputDecoration(
            hintText: loc.invitationCodeScreen_inputHint,
            fillColor: colors.backgroundBase.tertiary,
            helperStyle: TextStyle(
              fontSize: LabelFontSize.small2.size,
              color: colors.text.quaternary,
            ),
          ),
          maxLength: 8,
          inputFormatters: [
            inputFormatter,
            LengthLimitingTextInputFormatter(8),
          ],
          textCapitalization: TextCapitalization.characters,
          keyboardType: TextInputType.visiblePassword,
          onChanged: (value) {
            context.read<RegistrationCubit>().setInvitationCode(value);
          },
          onFieldSubmitted: (_) {
            focusNode.requestFocus();
            onFieldSubmitted();
          },
          validator: (value) {
            final code = context.read<RegistrationCubit>().state.invitationCode;
            if (code == null || code.length != 8) {
              return loc.invitationCodeScreen_error_invalidLength;
            }
            return null;
          },
        ),
      ],
    );
  }
}

class _JoinButton extends StatelessWidget {
  const _JoinButton({required this.formKey, required this.showErrors});

  final GlobalKey<FormState> formKey;
  final ValueNotifier<bool> showErrors;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);
    final isCheckingInvitationCode = context.select(
      (RegistrationCubit cubit) => cubit.state.isCheckingInvitationCode,
    );

    return OutlinedButton(
      style: OutlinedButtonTheme.of(context).style!.copyWith(
        backgroundColor: WidgetStateProperty.all(colors.accent.primary),
        foregroundColor: WidgetStateProperty.all(colors.function.toggleWhite),
      ),
      onPressed: isCheckingInvitationCode
          ? null
          : () {
              showErrors.value = true;
              if (!formKey.currentState!.validate()) {
                return;
              }
              _submit(context, formKey);
            },
      child: isCheckingInvitationCode
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
              loc.invitationCodeScreen_actionButton,
              style: TextStyle(
                color: colors.function.toggleWhite,
                fontSize: LabelFontSize.base.size,
              ),
            ),
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

void _submit(BuildContext context, GlobalKey<FormState> formKey) async {
  if (!formKey.currentState!.validate()) {
    return;
  }

  final navigationCubit = context.read<NavigationCubit>();
  final registrationCubit = context.read<RegistrationCubit>();

  final error = await registrationCubit.submitInvitationCode();

  if (error == null) {
    navigationCubit.openIntroScreen(const IntroScreenType.signUp());
  } else if (context.mounted) {
    final loc = AppLocalizations.of(context);
    final message = switch (error.code) {
      .missing => loc.invitationCodeScreen_error_missing,
      .invalid => loc.invitationCodeScreen_error_invalid,
      .internal => loc.invitationCodeScreen_error_internal(error.message ?? ""),
    };
    showErrorBannerStandalone(message);
  }
}
