// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/delivery_status/delivery_status.dart';
import 'package:air/ds/patterns/chat_list/chat_list_item_tokens.dart';
import 'package:flutter/widgets.dart';

/// How far the chat's last own message got, shown in a row's trailing gutter
/// where there's no unread count to show instead.
///
/// The glyph keeps its footprint through the in-flight delay, so the row
/// doesn't reflow when the spinner arrives.
class ChatListStatusIndicator extends StatelessWidget {
  const ChatListStatusIndicator({super.key, required this.status});

  final MessageDeliveryStatus status;

  @override
  Widget build(BuildContext context) {
    return DeliveryStatus(
      status: status,
      size: ChatListItemTokens.statusIconSize,
      holdsSpace: true,
    );
  }
}
