// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/theme/theme.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/components/modal/app_dialog.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
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
    final colors = CustomColorScheme.of(context);

    final description = this.description;
    final maxLength = this.maxLength;

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              title,
              style: TextStyle(
                fontSize: HeaderFontSize.h4.size,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          const SizedBox(height: Spacing.px24),

          TextFormField(
            autocorrect: false,
            autofocus: true,
            controller: controller,
            focusNode: focusNode,
            maxLength: maxLength,
            // Hide the built-in counter - we render our own below.
            buildCounter:
                (_, {required currentLength, required isFocused, maxLength}) =>
                    null,
            decoration: appDialogInputDecoration.copyWith(
              filled: true,
              fillColor: colors.backgroundBase.secondary,
            ),
            onChanged: (value) {
              isValid.value = validator(value);
              length.value = value.characters.length;
            },
            onFieldSubmitted: (_) {
              focusNode.requestFocus();
              onSubmit(controller.text);
            },
          ),

          const SizedBox(height: Spacing.px12),

          if (maxLength != null) ...[
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Spacing.px8),
              child: Text(
                loc.editDialog_characters_remaining(length.value, maxLength),
                style: TextStyle(
                  color: colors.text.tertiary,
                  fontSize: BodyFontSize.small2.size,
                ),
              ),
            ),
            const SizedBox(height: Spacing.px12),
          ],

          if (description != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Spacing.px8),
              child: Text(
                description,
                style: TextStyle(
                  color: colors.text.tertiary,
                  fontSize: BodyFontSize.small2.size,
                ),
              ),
            ),

          const SizedBox(height: Spacing.px12),

          Row(
            children: [
              Expanded(
                child: AppButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  type: .secondary,
                  label: cancel,
                ),
              ),

              const SizedBox(width: Spacing.px12),

              Expanded(
                child: AppButton(
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
