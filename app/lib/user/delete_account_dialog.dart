// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/main.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/icons/app_icon.dart';
import 'package:air/user/user.dart';
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

    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              loc.deleteAccountScreen_title,
              style: TextStyle(
                fontSize: HeaderFontSize.h4.size,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
          const SizedBox(height: Spacings.m),

          Center(
            child: AppIcon(
              type: AppIconType.warningCircle,
              size: 40,
              color: colors.function.danger,
            ),
          ),

          const SizedBox(height: Spacings.m),

          Text(
            loc.deleteAccountScreen_explanatoryText,
            style: TextStyle(
              color: colors.text.secondary,
              fontSize: BodyFontSize.base.size,
            ),
          ),

          const SizedBox(height: Spacings.xs),

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

          const SizedBox(height: Spacings.xs),

          Padding(
            padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
            child: Text(
              loc.deleteAccountScreen_confirmationInputLabel,
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
                  ),
                  child: Text(loc.editDisplayNameScreen_cancel),
                ),
              ),

              const SizedBox(width: Spacings.xs),

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
                          ? colors.function.white.withValues(alpha: 0.7)
                          : colors.function.white,
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
    isDeleting.value = true;
    final userCubit = context.read<UserCubit>();
    final coreClient = context.read<CoreClient>();
    try {
      await userCubit.deleteAccount(confirmationText: confirmationText);
      coreClient.logout();
    } catch (e) {
      _log.severe("Failed to delete account: $e");
      if (context.mounted) {
        final loc = AppLocalizations.of(context);
        showErrorBanner(context, loc.deleteAccountScreen_deleteAccountError);
      }
    } finally {
      isDeleting.value = false;
    }
  }
}
