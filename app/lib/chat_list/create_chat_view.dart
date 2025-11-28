// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/user_handle_input_formatter.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

class CreateChatView extends HookWidget {
  final String title;
  final String prompt;
  final String hint;
  final String action;
  final Future<String?> Function(String)? onAction;
  final FormFieldValidator<String> validator;
  final bool allowUnderscore;

  @override
  const CreateChatView(
    BuildContext context,
    this.title,
    this.prompt,
    this.hint,
    this.action, {
    this.onAction,
    required this.validator,
    this.allowUnderscore = false,
    super.key,
  });

  @override
  Widget build(context) {
    final formKey = useMemoized(() => GlobalKey<FormState>());

    final isInputValid = useState(false);
    final customValidationError = useState<String?>(null);

    final controller = useTextEditingController();

    final focusNode = useFocusNode();

    return AlertDialog(
      title: Text(title),
      titlePadding: const EdgeInsets.all(20),
      titleTextStyle: Theme.of(context).textTheme.titleLarge?.copyWith(
        color: CustomColorScheme.of(context).text.secondary,
      ),
      actionsAlignment: MainAxisAlignment.spaceBetween,
      actionsPadding: const EdgeInsets.all(20),
      buttonPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 20),
      contentPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 10),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const SizedBox(height: 50),
              Text(prompt, style: Theme.of(context).textTheme.bodyMedium),
              const SizedBox(height: 20),
              Form(
                key: formKey,
                autovalidateMode: AutovalidateMode.onUserInteraction,
                child: ConstrainedBox(
                  constraints: BoxConstraints.tight(const Size(380, 80)),
                  child: TextFormField(
                    autofocus: true,
                    controller: controller,
                    focusNode: focusNode,
                    decoration: InputDecoration(hintText: hint),
                    inputFormatters: [
                      UserHandleInputFormatter(
                        allowUnderscore: allowUnderscore,
                      ),
                    ],
                    onChanged: (text) => customValidationError.value = null,
                    validator: (input) =>
                        _validator(isInputValid, customValidationError, input),
                    onFieldSubmitted: (text) {
                      // keep focus on the input field
                      focusNode.requestFocus();
                      _onAction(
                        context,
                        formKey,
                        isInputValid,
                        customValidationError,
                        text,
                      );
                    },
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
      actions: <Widget>[
        TextButton(
          onPressed: () {
            Navigator.of(context).pop();
          },
          child: const Text('Cancel'),
        ),
        TextButton(
          onPressed: isInputValid.value
              ? () => _onAction(
                  context,
                  formKey,
                  isInputValid,
                  customValidationError,
                  controller.text,
                )
              : null,
          child: Text(action),
        ),
      ],
    );
  }

  void _onAction(
    BuildContext context,
    GlobalKey<FormState> formKey,
    ValueNotifier<bool> isInputValid,
    ValueNotifier<String?> customValidationError,
    String value,
  ) async {
    if (!isInputValid.value) {
      return;
    }

    if (onAction != null) {
      final error = await onAction!(value);
      if (error != null) {
        isInputValid.value = false;
        customValidationError.value = error;
        formKey.currentState?.validate();
      }
    } else {
      Navigator.of(context).pop(value);
    }
  }

  String? _validator(
    ValueNotifier<bool> isInputValid,
    ValueNotifier<String?> customValidationError,
    String? input,
  ) {
    final error = customValidationError.value ?? validator(input);
    final isValid = error == null;
    if (isValid != isInputValid.value) {
      // Don't update the value in the same build frame
      scheduleMicrotask(() => isInputValid.value = isValid);
    }
    return error;
  }
}
