// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:air/ds/foundations/icons.dart';
import 'package:air/ds/foundations/type_scale.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/dimensions.dart';
import 'package:air/ds/foundations/semantic_colors.dart';
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

    final controller = useTextEditingController(
      text: (this.isConfirmed) ? _confirmationText : "",
    );

    final colors = SemanticColors.of(context);
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
            child: AppIcon.circleAlert(size: 40, color: colors.function.danger),
          ),

          const SizedBox(height: S.s24),

          Text(
            loc.deleteAccountScreen_explanatoryText,
            style: typeScale.body.regular.style(color: colors.text.secondary),
          ),

          const SizedBox(height: S.s12),

          TextFormField(
            autocorrect: false,
            autofocus: true,
            controller: controller,
            decoration: appDialogInputDecoration.copyWith(
              hintText: loc.deleteAccountScreen_confirmationInputHint,
              filled: true,
              fillColor: colors.backgroundBase.secondary,
            ),
            onChanged: (value) =>
                isConfirmed.value = value == _confirmationText,
          ),

          const SizedBox(height: S.s12),

          Padding(
            padding: const EdgeInsets.symmetric(horizontal: S.s8),
            child: Text(
              loc.deleteAccountScreen_confirmationInputLabel,
              style: typeScale.body.xs.style(color: colors.text.tertiary),
            ),
          ),

          const SizedBox(height: S.s24),

          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accentBrand.quaternary,
                    ),
                  ),
                  child: Text(loc.editDisplayNameScreen_cancel),
                ),
              ),

              const SizedBox(width: S.s12),

              Expanded(
                child: AppDialogProgressButton(
                  onPressed: isConfirmed.value
                      ? (inProgress) =>
                            _deleteAccount(context, inProgress, controller.text)
                      : null,
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.function.danger,
                    ),
                    foregroundColor: WidgetStateProperty.resolveWith(
                      (states) => states.contains(WidgetState.disabled)
                          ? colors.function.neutral.white.withValues(alpha: 0.7)
                          : colors.function.neutral.white,
                    ),
                  ),
                  child: Text(loc.deleteAccountScreen_confirmButtonText),
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
    ValueNotifier<bool> isDeleting,
    String confirmationText,
  ) async {
    AppHaptics.destructive();
    isDeleting.value = true;
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
      isDeleting.value = false;
    }
  }
}
