// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/ds/patterns/dialog/dialog_tokens.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// A dialog for editing a single text value
class EditDialog extends HookWidget {
  const EditDialog({
    super.key,

    required this.title,
    this.description,
    required this.cancel,
    required this.confirm,
    required this.initialValue,

    required this.validator,
    required this.onSubmit,

    this.maxLength,
  });

  final String title;
  final String? description;
  final String cancel;
  final String confirm;

  final bool Function(String) validator;
  final Function(String) onSubmit;

  final String initialValue;

  /// When set, caps the input at this many characters and shows a live
  /// remaining-characters counter below the field.
  final int? maxLength;

  @override
  Widget build(BuildContext context) {
    final isValid = useState(validator(initialValue));

    final controller = useTextEditingController(text: initialValue);
    final focusNode = useFocusNode();
    final length = useState(initialValue.characters.length);

    final loc = AppLocalizations.of(context);
    final tokens = DialogTokens.of(context);
    final palette = SemanticPalette.of(context);

    final description = this.description;
    final maxLength = this.maxLength;

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            title,
            textAlign: TextAlign.center,
            style: typeScale.header.regular.style(
              color: palette.text.primary,
              weight: Weight.emphasized,
            ),
          ),
          SizedBox(height: tokens.titleBodyGap),

          AppTextInput(
            tokens: AppTextInputTokens.of(context),
            autocorrect: false,
            autofocus: true,
            controller: controller,
            focusNode: focusNode,
            maxLength: maxLength,
            onChanged: (value) {
              isValid.value = validator(value);
              length.value = value.characters.length;
            },
            onSubmitted: (_) {
              focusNode.requestFocus();
              onSubmit(controller.text);
            },
          ),

          // The field caps the length silently, so the readout is the dialog's
          // own line, styled to match the description it sits above.
          if (maxLength != null) ...[
            const SizedBox(height: S.s12),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: S.s8),
              child: Text(
                loc.editDialog_characters_remaining(length.value, maxLength),
                style: typeScale.body.xs.style(color: palette.text.tertiary),
              ),
            ),
          ],

          if (description != null) ...[
            const SizedBox(height: S.s12),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: S.s8),
              child: Text(
                description,
                style: typeScale.body.xs.style(color: palette.text.tertiary),
              ),
            ),
          ],

          SizedBox(height: tokens.bodyActionsGap),

          Row(
            children: [
              Expanded(
                child: Button(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  type: .secondary,
                  label: cancel,
                ),
              ),

              const SizedBox(width: S.s12),

              Expanded(
                child: Button(
                  onPressed: () => {
                    if (isValid.value) {onSubmit(controller.text)},
                  },
                  type: .primary,
                  label: confirm,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
