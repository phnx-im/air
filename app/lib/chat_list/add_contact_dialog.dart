// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/username_input_formatter.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

import 'chat_list_cubit.dart';

final _log = Logger("AddContactDialog");

class AddContactDialog extends HookWidget {
  const AddContactDialog({super.key});

  @override
  Widget build(context) {
    final formKey = useMemoized(() => GlobalKey<FormState>());

    final usernameHash = useState<UsernameHash?>(null);
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
              inputFormatters: const [UsernameInputFormatter()],
              decoration: appDialogInputDecoration.copyWith(
                hintText: loc.newConnectionDialog_usernamePlaceholder,
                filled: true,
                fillColor: colors.backgroundBase.secondary,
              ),
              onChanged: (value) {
                errorMessage.value = null;
                usernameHash.value = null;
                isInputValid.value = _validate(value);
              },
              onFieldSubmitted: (value) {
                focusNode.requestFocus();
                _SubmitHandler(
                  formKey: formKey,
                  isSubmitting: isSubmitting,
                  errorMessage: errorMessage,
                  usernameHash: usernameHash,
                  value: value,
                )._submit(context);
              },
            ),

            const SizedBox(height: Spacings.xs),

            Container(
              constraints: const BoxConstraints(minHeight: 38),
              padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
              child: _Description(
                hasUsernameHash: usernameHash.value != null,
                errorMessage: errorMessage.value,
                username: controller.text,
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
                      usernameHash: usernameHash,
                      value: controller.text,
                    )._submit(context),
                    state: isSubmitting.value
                        ? .pending
                        : isInputValid.value && errorMessage.value == null
                        ? .active
                        : .inactive,
                    label: usernameHash.value == null
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
    final normalized = UsernameInputFormatter.normalize(value);
    if (normalized.isEmpty) {
      return false;
    }
    UiUsername username = UiUsername(plaintext: normalized);
    return username.validationError() == null;
  }
}

class _Description extends StatelessWidget {
  const _Description({
    required this.hasUsernameHash,
    required this.errorMessage,
    required this.username,
  });

  final bool hasUsernameHash;
  final String? errorMessage;
  final String username;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final colors = CustomColorScheme.of(context);

    final (text, color) = switch ((errorMessage, hasUsernameHash)) {
      (final errorMessage?, _) => (errorMessage, colors.function.danger),
      (null, true) => (
        loc.newConnectionDialog_handleExists(username),
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
    required this.usernameHash,
    required this.value,
  });

  final GlobalKey<FormState> formKey;
  final ValueNotifier<bool> isSubmitting;
  final ValueNotifier<String?> errorMessage;
  final ValueNotifier<UsernameHash?> usernameHash;
  final String value;

  void _submit(BuildContext context) {
    if (!formKey.currentState!.validate()) {
      return;
    }

    isSubmitting.value = true;

    final normalized = UsernameInputFormatter.normalize(value);

    final loc = AppLocalizations.of(context);
    if (normalized.isEmpty) {
      errorMessage.value = loc.newConnectionDialog_error_emptyUsername;
      isSubmitting.value = false;
      return;
    }
    final username = UiUsername(plaintext: normalized);

    if (usernameHash.value == null) {
      _checkUsername(context, username);
    } else {
      _connectUsername(context, username, usernameHash.value!);
    }
  }

  void _checkUsername(BuildContext context, UiUsername username) async {
    isSubmitting.value = true;
    final userCubit = context.read<UserCubit>();
    final hash = await userCubit.checkUsernameExists(username: username);

    if (!context.mounted) return;
    final loc = AppLocalizations.of(context);

    if (hash == null) {
      errorMessage.value = loc.newConnectionDialog_error_usernameNotFound(
        username.plaintext,
      );
      isSubmitting.value = false;
      return;
    }

    usernameHash.value = hash;
    isSubmitting.value = false;
  }

  void _connectUsername(
    BuildContext context,
    UiUsername username,
    UsernameHash hash,
  ) async {
    isSubmitting.value = true;

    final loc = AppLocalizations.of(context);
    final chatListCubit = context.read<ChatListCubit>();
    try {
      final error = await chatListCubit.createContactChat(
        username: username,
        hash: hash,
      );
      final errorMessage = switch (error) {
        AddUsernameContactError.usernameNotFound =>
          loc.newConnectionDialog_error_usernameNotFound(username.plaintext),
        AddUsernameContactError.duplicateRequest =>
          loc.newConnectionDialog_error_duplicateRequest,
        AddUsernameContactError.ownUsername =>
          loc.newConnectionDialog_error_ownUsername,
        null => null,
      };
      if (errorMessage != null) {
        this.errorMessage.value = errorMessage;
      } else if (context.mounted) {
        Navigator.of(context).pop();
      }
    } catch (e) {
      // fatal error
      _log.severe("Failed to create connection: $e", e);
      showErrorBannerStandalone(
        (loc) => loc.newConnectionDialog_error(username.plaintext),
      );
    } finally {
      isSubmitting.value = false;
    }
  }
}
