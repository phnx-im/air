// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// A dialog for editing a single text value
class EditDialog extends HookWidget {
  const EditDialog({
    super.key,

    required this.title,
    required this.description,
    required this.cancel,
    required this.confirm,
    required this.initialValue,

    required this.validator,
    required this.onSubmit,
  });

  final String title;
  final String description;
  final String cancel;
  final String confirm;

  final bool Function(String) validator;
  final Function(String) onSubmit;

  final String initialValue;

  @override
  Widget build(BuildContext context) {
    final isValid = useState(validator(initialValue));

    final controller = useTextEditingController(text: initialValue);
    final focusNode = useFocusNode();

    final colors = CustomColorScheme.of(context);

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
          const SizedBox(height: Spacings.m),

          TextFormField(
            autocorrect: false,
            autofocus: true,
            controller: controller,
            focusNode: focusNode,
            decoration: appDialogInputDecoration.copyWith(
              filled: true,
              fillColor: colors.backgroundBase.secondary,
            ),
            onChanged: (value) {
              isValid.value = validator(value);
            },
            onFieldSubmitted: (_) {
              focusNode.requestFocus();
              onSubmit(controller.text);
            },
          ),

          const SizedBox(height: Spacings.xs),

          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
            child: Text(
              description,
              style: TextStyle(
                color: colors.text.tertiary,
                fontSize: BodyFontSize.small2.size,
              ),
            ),
          ),

          const SizedBox(height: Spacings.m),

          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accent.quaternary,
                    ),
                    overlayColor: WidgetStatePropertyAll(
                      colors.accent.quaternary,
                    ),
                  ),
                  child: Text(cancel),
                ),
              ),

              const SizedBox(width: Spacings.xs),

              Expanded(
                child: OutlinedButton(
                  onPressed: isValid.value
                      ? () => onSubmit(controller.text)
                      : null,
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accent.primary,
                    ),
                    overlayColor: WidgetStatePropertyAll(colors.accent.primary),
                    foregroundColor: WidgetStateProperty.resolveWith(
                      (states) => states.contains(WidgetState.disabled)
                          ? colors.function.toggleWhite.withValues(alpha: 0.5)
                          : colors.function.toggleWhite,
                    ),
                  ),
                  child: Text(confirm),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
