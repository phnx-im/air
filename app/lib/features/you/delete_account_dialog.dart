// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

const _confirmationText = 'delete';

final _log = Logger("DeleteAccountDialog");

class DeleteAccountDialog extends HookWidget {
  const DeleteAccountDialog({super.key, this.isConfirmed = false});

  final bool isConfirmed;

  @override
  Widget build(BuildContext context) {
    final isConfirmed = useState(this.isConfirmed);
    final isDeleting = useState(false);

    final controller = useTextEditingController(
      text: (this.isConfirmed) ? _confirmationText : "",
    );

    final palette = SemanticPalette.of(context);
    final loc = AppLocalizations.of(context);

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              loc.deleteAccountScreen_title,
              style: typeScale.header.regular.style(weight: Weight.emphasized),
            ),
          ),
          const SizedBox(height: S.s24),

          Center(
            child: AppIcon.circleAlert(
              size: 40,
              color: palette.function.danger,
            ),
          ),

          const SizedBox(height: S.s24),

          Text(
            loc.deleteAccountScreen_explanatoryText,
            style: typeScale.body.regular.style(color: palette.text.secondary),
          ),

          const SizedBox(height: S.s12),

          AppTextInput(
            tokens: AppTextInputTokens.of(context),
            autocorrect: false,
            autofocus: true,
            controller: controller,
            hintText: loc.deleteAccountScreen_confirmationInputHint,
            helperText: loc.deleteAccountScreen_confirmationInputLabel,
            onChanged: (value) =>
                isConfirmed.value = value == _confirmationText,
          ),

          const SizedBox(height: S.s24),

          Row(
            children: [
              Expanded(
                child: Button(
                  onPressed: () => Navigator.of(context).pop(false),
                  label: loc.editDisplayNameScreen_cancel,
                  type: ButtonType.secondary,
                ),
              ),

              const SizedBox(width: S.s12),

              Expanded(
                child: Button(
                  onPressed: () => _deleteAccount(
                    context,
                    (value) => isDeleting.value = value,
                    controller.text,
                  ),
                  label: loc.deleteAccountScreen_confirmButtonText,
                  tone: ButtonTone.danger,
                  state: switch ((isDeleting.value, isConfirmed.value)) {
                    (true, _) => ButtonState.pending,
                    (false, true) => ButtonState.active,
                    (false, false) => ButtonState.inactive,
                  },
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _deleteAccount(
    BuildContext context,
    void Function(bool) setDeleting,
    String confirmationText,
  ) async {
    AppHaptics.destructive();
    setDeleting(true);
    final userCubit = context.read<UserCubit>();
    final coreClient = context.read<CoreClient>();
    try {
      await userCubit.deleteAccount(confirmationText: confirmationText);
      coreClient.logout();
    } catch (e) {
      _log.severe("Failed to delete account: $e", e);
      showErrorBannerStandalone(
        (loc) => loc.deleteAccountScreen_deleteAccountError,
      );
    } finally {
      setDeleting(false);
    }
  }
}
