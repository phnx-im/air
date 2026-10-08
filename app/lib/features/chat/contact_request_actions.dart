// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/haptics.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart' show showDialog;
import 'package:flutter/widgets.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

final _log = Logger("ContactRequestActions");

/// Retracts the outgoing contact request of [chatId] once the user confirms.
/// The chat disappears with it.
Future<void> retractContactRequest(BuildContext context, ChatId chatId) async {
  final userCubit = context.read<UserCubit>();
  final navigationCubit = context.read<NavigationCubit>();
  final loc = AppLocalizations.of(context);
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => ConfirmDialog(
      title: loc.retractContactRequestDialog_title,
      message: loc.retractContactRequestDialog_content,
      cancel: loc.retractContactRequestDialog_cancel,
      confirm: loc.retractContactRequestDialog_confirm,
      destructive: true,
    ),
  );
  if (!(confirmed ?? false)) return;
  AppHaptics.destructive();
  try {
    await userCubit.retractContactRequest(chatId);
  } catch (e, stackTrace) {
    _log.severe("Failed to retract contact request: $e", e, stackTrace);
    showSnackBarStandalone(
      (loc) => loc.sentContactRequest_error_retract,
      tone: .danger,
    );
    return;
  }
  // Another chat open next to the chat list stays.
  if (navigationCubit.state.chatId == chatId) {
    navigationCubit.closeChat();
  }
}

/// Deletes the chat of a contact request that ended without a connection,
/// once the user confirms.
Future<void> deleteClosedContactRequest(
  BuildContext context,
  ChatId chatId,
) async {
  final userCubit = context.read<UserCubit>();
  final navigationCubit = context.read<NavigationCubit>();
  final loc = AppLocalizations.of(context);
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => ConfirmDialog(
      title: loc.deleteChatDialog_title,
      message: loc.closedContactRequest_deleteDialog_content,
      cancel: loc.closedContactRequest_deleteDialog_cancel,
      confirm: loc.deleteChatDialog_delete,
      destructive: true,
    ),
  );
  if (!(confirmed ?? false)) return;
  AppHaptics.destructive();
  // The chat disappears, so it must not stay open. Another chat open next to
  // the chat list stays.
  if (navigationCubit.state.chatId == chatId) {
    navigationCubit.closeChat();
  }
  try {
    await userCubit.deleteChat(chatId);
  } catch (e, stackTrace) {
    _log.severe("Failed to delete contact request: $e", e, stackTrace);
    showSnackBarStandalone(
      (loc) => loc.closedContactRequest_error_delete,
      tone: .danger,
    );
  }
}
