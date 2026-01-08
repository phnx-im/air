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
import 'package:air/user/user.dart';
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

    final handleHash = useState<UserHandleHash?>(null);
    final isSubmitting = useState(false);
    final isInputValid = useState(false);
    final errorMessage = useState<String?>(null);

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
                errorMessage.value = null;
                handleHash.value = null;
                isInputValid.value = _validate(value);
              },
              onFieldSubmitted: (value) {
                focusNode.requestFocus();
                _SubmitHandler(
                  formKey: formKey,
                  isSubmitting: isSubmitting,
                  errorMessage: errorMessage,
                  handleHash: handleHash,
                  value: value,
                )._submit(context);
              },
            ),

            const SizedBox(height: Spacings.xs),

            Container(
              constraints: const BoxConstraints(minHeight: 38),
              padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
              child: _Description(
                hasHandleHash: handleHash.value != null,
                errorMessage: errorMessage.value,
                handle: controller.text,
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
                    onPressed: () => _SubmitHandler(
                      formKey: formKey,
                      isSubmitting: isSubmitting,
                      errorMessage: errorMessage,
                      handleHash: handleHash,
                      value: controller.text,
                    )._submit(context),
                    state: isSubmitting.value
                        ? .pending
                        : isInputValid.value && errorMessage.value == null
                        ? .active
                        : .inactive,
                    label: handleHash.value == null
                        ? loc.newConnectionDialog_confirm1
                        : loc.newConnectionDialog_confirm2,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  bool _validate(String value) {
    final normalized = UserHandleInputFormatter.normalize(value);
    if (normalized.isEmpty) {
      return false;
    }
    UiUserHandle handle = UiUserHandle(plaintext: normalized);
    return handle.validationError() == null;
  }
}

class _Description extends StatelessWidget {
  const _Description({
    required this.hasHandleHash,
    required this.errorMessage,
    required this.handle,
  });

  final bool hasHandleHash;
  final String? errorMessage;
  final String handle;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final (text, color) = switch ((errorMessage, hasHandleHash)) {
      (final errorMessage?, _) => (errorMessage, colors.function.danger),
      (null, true) => (
        loc.newConnectionDialog_handleExists(handle),
        colors.function.success,
      ),
      (null, false) => (
        loc.newConnectionDialog_newConnectionDescription,
        colors.text.tertiary,
      ),
    };

    return Text(
      text,
      style: TextStyle(color: color, fontSize: BodyFontSize.small2.size),
    );
  }
}

class _SubmitHandler {
  const _SubmitHandler({
    required this.formKey,
    required this.isSubmitting,
    required this.errorMessage,
    required this.handleHash,
    required this.value,
  });

  final GlobalKey<FormState> formKey;
  final ValueNotifier<bool> isSubmitting;
  final ValueNotifier<String?> errorMessage;
  final ValueNotifier<UserHandleHash?> handleHash;
  final String value;

  void _submit(BuildContext context) {
    if (!formKey.currentState!.validate()) {
      return;
    }

    isSubmitting.value = true;

    final normalized = UserHandleInputFormatter.normalize(value);

    final loc = AppLocalizations.of(context);
    if (normalized.isEmpty) {
      errorMessage.value = loc.newConnectionDialog_error_emptyHandle;
      isSubmitting.value = false;
      return;
    }
    final handle = UiUserHandle(plaintext: normalized);

    if (handleHash.value == null) {
      _checkHandle(context, handle);
    } else {
      _connectHandle(context, handle, handleHash.value!);
    }
  }

  void _checkHandle(BuildContext context, UiUserHandle handle) async {
    isSubmitting.value = true;
    final userCubit = context.read<UserCubit>();
    final hash = await userCubit.checkHandleExists(handle: handle);

    if (!context.mounted) return;
    final loc = AppLocalizations.of(context);

    if (hash == null) {
      errorMessage.value = loc.newConnectionDialog_error_handleNotFound(
        handle.plaintext,
      );
      isSubmitting.value = false;
      return;
    }

    handleHash.value = hash;
    isSubmitting.value = false;
  }

  void _connectHandle(
    BuildContext context,
    UiUserHandle handle,
    UserHandleHash hash,
  ) async {
    isSubmitting.value = true;

    final loc = AppLocalizations.of(context);
    final chatListCubit = context.read<ChatListCubit>();
    try {
      final result = await chatListCubit.createContactChat(
        handle: handle,
        hash: hash,
      );
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
          this.errorMessage.value = errorMessage;
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
}
