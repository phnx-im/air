// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

import 'user_cubit.dart';

const _confirmationText = "delete";

final _log = Logger('DeleteAccountScreen');

class DeleteAccountScreen extends HookWidget {
  const DeleteAccountScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final isConfirmed = useState(false);

    final loc = AppLocalizations.of(context);

    final dangerColor = CustomColorScheme.of(context).function.danger;

    return Scaffold(
      appBar: AppBar(
        title: Text(loc.deleteAccountScreen_title),
        leading: const AppBarBackButton(),
      ),
      body: SafeArea(
        minimum: const EdgeInsets.only(
          left: Spacings.s,
          right: Spacings.s,
          bottom: Spacings.m,
        ),
        child: Align(
          alignment: Alignment.topCenter,
          child: Container(
            constraints:
                isPointer() ? const BoxConstraints(maxWidth: 800) : null,
            child: Column(
              mainAxisSize: MainAxisSize.max,
              children: [
                Expanded(
                  child: ListView(
                    padding: EdgeInsets.zero,
                    children: [
                      Icon(Icons.warning_rounded, color: dangerColor, size: 120),
                      const SizedBox(height: Spacings.s),
                      Text(
                        loc.deleteAccountScreen_explanatoryText,
                        textAlign: TextAlign.center,
                        style: Theme.of(context).textTheme.bodyMedium,
                      ),
                      const SizedBox(height: Spacings.l),
                      TextField(
                        onChanged:
                            (value) =>
                                isConfirmed.value = value == _confirmationText,
                        decoration: InputDecoration(
                          hintText:
                              loc.deleteAccountScreen_confirmationInputHint,
                        ),
                      ),
                      const SizedBox(height: Spacings.s),
                      Align(
                        alignment: Alignment.centerLeft,
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: Spacings.xxs,
                          ),
                          child: Text(
                            style: TextStyle(color: Theme.of(context).hintColor),
                            loc.deleteAccountScreen_confirmationInputLabel,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: Spacings.m),
                Wrap(
                  runSpacing: Spacings.xs,
                  spacing: Spacings.xs,
                  runAlignment: WrapAlignment.center,
                  alignment: WrapAlignment.center,
                  children: [
                    SizedBox(
                      width: isSmallScreen(context) ? double.infinity : null,
                      child: _DeleteAccountButton(
                        isConfirmed: isConfirmed.value,
                      ),
                    ),
                    SizedBox(
                      width: isSmallScreen(context) ? double.infinity : null,
                      child: OutlinedButton(
                        onPressed: () {
                          Navigator.of(context).pop();
                        },
                        child: Text(
                          loc.deleteAccountScreen_cancelButtonText,
                        ),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _DeleteAccountButton extends StatelessWidget {
  const _DeleteAccountButton({required this.isConfirmed});

  final bool isConfirmed;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final dangerColor = CustomColorScheme.of(context).function.danger;
    return ProgressButton(
      style: OutlinedButton.styleFrom(
        backgroundColor: dangerColor,
        disabledBackgroundColor: dangerColor.withValues(alpha: 0.7),
        overlayColor: dangerColor,
      ),
      onPressed:
          isConfirmed
              ? (isDeleting) => _deleteAccount(context, isDeleting)
              : null,
      label: loc.deleteAccountScreen_confirmButtonText,
    );
  }

  void _deleteAccount(
    BuildContext context,
    ValueNotifier<bool> isDeleting,
  ) async {
    _log.info("Deleting account");
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
