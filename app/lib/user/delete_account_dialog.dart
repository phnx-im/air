// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/main.dart';
import 'package:air/ui/colors/palette.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/user/user.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;

const _confirmationText = 'delete';

final _log = Logger("DeleteAccountDialog");

class DeleteAccountDialog extends HookWidget {
  const DeleteAccountDialog({super.key});

  @override
  Widget build(BuildContext context) {
    final isConfirmed = useState(false);

    final controller = useTextEditingController();

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
            child: iconoir.WarningCircle(
              width: 40,
              height: 40,
              color: colors.function.danger,
            ),
          ),

          const SizedBox(height: Spacings.s),

          FieldLabel(loc.deleteAccountScreen_explanatoryText),

          const SizedBox(height: Spacings.xs),

          TextFormField(
            autocorrect: false,
            autofocus: true,
            controller: controller,
            decoration: airInputDecoration.copyWith(
              hintText: loc.deleteAccountScreen_confirmationInputHint,
              filled: true,
              fillColor: colors.backgroundBase.secondary,
            ),
            onChanged: (value) =>
                isConfirmed.value = value == _confirmationText,
          ),

          const SizedBox(height: Spacings.xs),

          FieldLabel(loc.deleteAccountScreen_confirmationInputLabel),

          const SizedBox(height: Spacings.m),

          Row(
            children: [
              Expanded(
                child: TextButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  style: airDialogButtonStyle.copyWith(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accent.quaternary,
                    ),
                  ),
                  child: Text(
                    loc.editDisplayNameScreen_cancel,
                    style: TextStyle(fontSize: LabelFontSize.base.size),
                  ),
                ),
              ),

              const SizedBox(width: Spacings.xs),

              Expanded(
                child: AirDialogProgressTextButton(
                  onPressed: (inProgress) =>
                      _deleteAccount(context, inProgress),
                  style: airDialogButtonStyle.copyWith(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.function.danger,
                    ),
                    foregroundColor: WidgetStatePropertyAll(
                      AppColors.neutral[200],
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
  ) async {
    isDeleting.value = true;
    final userCubit = context.read<UserCubit>();
    final coreClient = context.read<CoreClient>();
    try {
      await userCubit.deleteAccount();
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
