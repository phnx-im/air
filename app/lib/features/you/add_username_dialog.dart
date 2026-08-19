// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/util/username_input_formatter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

import 'package:air/features/user/user_cubit.dart';

class AddUsernameDialog extends HookWidget {
  const AddUsernameDialog({super.key, this.inProgress});

  final bool? inProgress;

  @override
  Widget build(BuildContext context) {
    final usernameExists = useState(false);
    final isSubmitting = useState(inProgress ?? false);
    final errorText = useState<String?>(null);

    final controller = useTextEditingController();
    final focusNode = useFocusNode();

    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);
    final hintStyle = typeScale.body.xs.style(color: palette.text.tertiary);

    // The field carries no validator of its own, so the message is host state,
    // recomputed whenever the value or the taken-name state moves.
    bool validate() {
      final error = _validate(loc, usernameExists.value, controller.text);
      errorText.value = error;
      return error == null;
    }

    void submit() => _submit(
      context: context,
      controller: controller,
      usernameExists: usernameExists,
      setSubmitting: (value) => isSubmitting.value = value,
      validate: validate,
    );

    return AppDialog(
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .start,
        children: [
          Center(
            child: Text(
              loc.usernameScreen_title,
              style: typeScale.header.regular.style(weight: Weight.emphasized),
            ),
          ),
          const SizedBox(height: S.s24),

          AppTextInput(
            tokens: AppTextInputTokens.current,
            autocorrect: false,
            autofocus: true,
            controller: controller,
            focusNode: focusNode,
            inputFormatters: const [UsernameInputFormatter()],
            hintText: loc.usernameScreen_inputHint,
            errorText: errorText.value,
            onChanged: (_) {
              if (usernameExists.value) {
                usernameExists.value = false;
                validate();
              }
            },
            onSubmitted: (_) {
              focusNode.requestFocus();
              submit();
            },
          ),

          const SizedBox(height: S.s12),

          Padding(
            padding: const EdgeInsets.symmetric(horizontal: S.s8),
            child: Column(
              crossAxisAlignment: .start,
              children: [
                Text(loc.usernameScreen_description, style: hintStyle),
                const SizedBox(height: S.s8),
                Text(loc.usernameScreen_syntax, style: hintStyle),
              ],
            ),
          ),

          const SizedBox(height: S.s24),

          Row(
            children: [
              Expanded(
                child: Button(
                  onPressed: () => Navigator.of(context).pop(false),
                  label: loc.usernameScreen_cancel,
                  type: ButtonType.secondary,
                ),
              ),
              const SizedBox(width: S.s12),
              Expanded(
                child: Button(
                  onPressed: submit,
                  label: loc.usernameScreen_confirm,
                  state: isSubmitting.value
                      ? ButtonState.pending
                      : ButtonState.active,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _submit({
    required BuildContext context,
    required TextEditingController controller,
    required ValueNotifier<bool> usernameExists,
    required void Function(bool) setSubmitting,
    required bool Function() validate,
  }) async {
    if (!validate()) {
      return;
    }
    final normalized = UsernameInputFormatter.normalize(controller.text);
    final username = UiUsername(plaintext: normalized);
    final userCubit = context.read<UserCubit>();

    setSubmitting(true);
    if (!await userCubit.addUsername(username)) {
      usernameExists.value = true;
      setSubmitting(false);
      validate();
      return;
    }
    if (!context.mounted) return;
    Navigator.of(context).pop();
  }

  String? _validate(AppLocalizations loc, bool usernameExists, String value) {
    if (usernameExists) {
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
