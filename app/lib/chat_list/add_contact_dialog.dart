// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/user_handle_input_formatter.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

import 'chat_list_cubit.dart';

class AddContactDialog extends HookWidget {
  const AddContactDialog({super.key});

  @override
  Widget build(context) {
    final formKey = useMemoized(() => GlobalKey<FormState>());

    final isSubmitting = useState(false);
    final isInputValid = useState(false);
    final customValidationError = useState<String?>(null);

    final controller = useTextEditingController();

    final focusNode = useFocusNode();

    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    return AppDialog(
      child: Form(
        key: formKey,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Center(
              child: Text(
                loc.newConnectionDialog_newConnectionTitle,
                style: TextStyle(
                  fontSize: HeaderFontSize.h4.size,
                  fontWeight: FontWeight.bold,
                ),
              ),
            ),

            const SizedBox(height: Spacings.m),

            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
              child: Text(
                loc.newConnectionDialog_inputLabel,
                style: TextStyle(
                  fontSize: LabelFontSize.small2.size,
                  color: colors.text.quaternary,
                ),
              ),
            ),

            const SizedBox(height: Spacings.xxs),

            TextFormField(
              autocorrect: false,
              autofocus: true,
              controller: controller,
              focusNode: focusNode,
              inputFormatters: const [UserHandleInputFormatter()],
              decoration: appDialogInputDecoration.copyWith(
                hintText: loc.newConnectionDialog_usernamePlaceholder,
                filled: true,
                fillColor: colors.backgroundBase.secondary,
              ),
              onChanged: (value) {
                customValidationError.value = null;
                isInputValid.value = _validate(value);
              },
              onFieldSubmitted: (value) {
                focusNode.requestFocus();
                _submit(
                  context: context,
                  formKey: formKey,
                  isSubmitting: isSubmitting,
                  customValidationError: customValidationError,
                  value: value,
                );
              },
            ),

            const SizedBox(height: Spacings.xs),

            if (customValidationError.value == null)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
                child: Text(
                  loc.newConnectionDialog_newConnectionDescription,
                  style: TextStyle(
                    color: colors.text.tertiary,
                    fontSize: BodyFontSize.small2.size,
                  ),
                ),
              ),

            if (customValidationError.value case final errorMessage?)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
                child: Text(
                  errorMessage,
                  style: TextStyle(
                    color: colors.function.danger,
                    fontSize: BodyFontSize.small2.size,
                  ),
                ),
              ),

            const SizedBox(height: Spacings.m),

            Row(
              children: [
                Expanded(
                  child: AppButton(
                    onPressed: () {
                      Navigator.of(context).pop(false);
                    },
                    type: .secondary,
                    label: loc.newConnectionDialog_cancel,
                  ),
                ),
                const SizedBox(width: Spacings.xs),
                Expanded(
                  child: AppButton(
                    onPressed: () {
                      _submit(
                        context: context,
                        formKey: formKey,
                        isSubmitting: isSubmitting,
                        customValidationError: customValidationError,
                        value: controller.text,
                      );
                    },
                    state: isSubmitting.value
                        ? .pending
                        : isInputValid.value &&
                              customValidationError.value == null
                        ? .active
                        : .inactive,
                    label: loc.newConnectionDialog_actionButton,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  void _submit({
    required BuildContext context,
    required GlobalKey<FormState> formKey,
    required ValueNotifier<bool> isSubmitting,
    required ValueNotifier<String?> customValidationError,
    required String value,
  }) async {
    if (!formKey.currentState!.validate()) {
      return;
    }

    isSubmitting.value = true;

    final normalized = UserHandleInputFormatter.normalize(value);

    final loc = AppLocalizations.of(context);
    if (normalized.isEmpty) {
      customValidationError.value = loc.newConnectionDialog_error_emptyHandle;
      isSubmitting.value = false;
      return;
    }

    final chatListCubit = context.read<ChatListCubit>();
    final handle = UiUserHandle(plaintext: normalized);
    try {
      final result = await chatListCubit.createContactChat(handle: handle);
      switch (result) {
        case AddHandleContactResult_Ok():
          // success
          if (context.mounted) {
            Navigator.of(context).pop();
          }
        case AddHandleContactResult_Err(field0: final error):
          // error
          final errorMessage = switch (error) {
            AddHandleContactError.handleNotFound =>
              loc.newConnectionDialog_error_handleNotFound(handle.plaintext),
            AddHandleContactError.duplicateRequest =>
              loc.newConnectionDialog_error_duplicateRequest,
            AddHandleContactError.ownHandle =>
              loc.newConnectionDialog_error_ownHandle,
          };
          customValidationError.value = errorMessage;
      }
    } catch (e) {
      // fatal error
      Logger.detached(
        "AddContactDialog",
      ).severe("Failed to create connection: $e");
      showErrorBannerStandalone(
        (loc) => loc.newConnectionDialog_error(handle.plaintext),
      );
    } finally {
      isSubmitting.value = false;
    }
  }

  bool _validate(String value) {
    final normalized = UserHandleInputFormatter.normalize(
      value,
      allowUnderscore: false,
    );
    if (normalized.isEmpty) {
      return false;
    }
    UiUserHandle handle = UiUserHandle(plaintext: normalized);
    return handle.validationError() == null;
  }
}
