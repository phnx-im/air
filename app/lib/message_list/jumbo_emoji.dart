// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/api/markdown.dart';
import 'package:air/core/api/message_content.dart';

const jumboEmojiScale = 2.5;
const maxEmojiCount = 5;

// Comprehensive emoji regex covering:
// - Emoji sequences with ZWJ, skin tones, variation selectors
// - Flag sequences (regional indicator pairs)
// - Keycap sequences
// - Basic emoji codepoints
final _emojiPattern = RegExp(
  r'(?:'
  // Keycap sequences: digit/# /* + VS16? + combining enclosing keycap
  r'[#*0-9]\uFE0F?\u20E3'
  r'|'
  // Flag sequences: regional indicator pairs
  r'[\u{1F1E6}-\u{1F1FF}]{2}'
  r'|'
  // Emoji with optional modifiers, joined by ZWJ
  r'(?:'
  r'[\u{1F3F4}](?:\u{E0067}\u{E0062}(?:\u{E0065}\u{E006E}\u{E0067}|\u{E0073}\u{E0063}\u{E0074}|\u{E0077}\u{E006C}\u{E0073})\u{E007F})'
  r'|'
  r'(?:[\u{1F600}-\u{1F64F}]|[\u{1F300}-\u{1F5FF}]|[\u{1F680}-\u{1F6FF}]'
  r'|[\u{1F700}-\u{1F77F}]|[\u{1F780}-\u{1F7FF}]|[\u{1F800}-\u{1F8FF}]'
  r'|[\u{1F900}-\u{1F9FF}]|[\u{1FA00}-\u{1FA6F}]|[\u{1FA70}-\u{1FAFF}]'
  r'|[\u{2600}-\u{26FF}]|[\u{2700}-\u{27BF}]|[\u{FE00}-\u{FE0F}]'
  r'|[\u{231A}-\u{231B}]|[\u{23E9}-\u{23F3}]|[\u{23F8}-\u{23FA}]'
  r'|[\u{25AA}-\u{25AB}]|[\u{25B6}]|[\u{25C0}]|[\u{25FB}-\u{25FE}]'
  r'|\u{200D}|\u{2328}|\u{23CF}|\u{2934}|\u{2935}'
  r'|\u{3030}|\u{303D}|\u{3297}|\u{3299}'
  r'|\u{2139}|\u{2194}-\u{2199}|\u{21A9}-\u{21AA}'
  r'|\u{20E3}|\u{2122}|\u{2611}|\u{2614}-\u{2615}'
  r'|\u{2648}-\u{2653}|\u{267F}|\u{2693}|\u{26A1}'
  r'|\u{26AA}-\u{26AB}|\u{26BD}-\u{26BE}|\u{26C4}-\u{26C5}'
  r'|\u{26CE}|\u{26D4}|\u{26EA}|\u{26F2}-\u{26F3}'
  r'|\u{26F5}|\u{26FA}|\u{26FD}|\u{2702}|\u{2705}'
  r'|\u{2708}-\u{270D}|\u{270F}|\u{2712}|\u{2714}|\u{2716}'
  r'|\u{271D}|\u{2721}|\u{2728}|\u{2733}-\u{2734}|\u{2744}|\u{2747}'
  r'|\u{274C}|\u{274E}|\u{2753}-\u{2755}|\u{2757}'
  r'|\u{2763}-\u{2764}|\u{2795}-\u{2797}|\u{27A1}|\u{27B0}|\u{27BF}'
  r'|\u{00A9}|\u{00AE}'
  r')'
  // Optional variation selector, skin tone, ZWJ chains
  r'(?:\uFE0F)?'
  r'(?:[\u{1F3FB}-\u{1F3FF}])?'
  r'(?:\u200D'
  r'(?:[\u{1F600}-\u{1F64F}]|[\u{1F300}-\u{1F5FF}]|[\u{1F680}-\u{1F6FF}]'
  r'|[\u{1F900}-\u{1F9FF}]|[\u{1FA00}-\u{1FA6F}]|[\u{1FA70}-\u{1FAFF}]'
  r'|[\u{2600}-\u{26FF}]|[\u{2700}-\u{27BF}]|[\u{2640}\u{2642}\u{2695}\u{2696}\u{2708}])'
  r'(?:\uFE0F)?'
  r'(?:[\u{1F3FB}-\u{1F3FF}])?'
  r')*'
  r')'
  r')',
  unicode: true,
);

/// Returns true if the inline elements contain only 1-5 emoji
/// (with optional whitespace) and no other content.
bool isJumboEmoji(List<RangedInlineElement> inlines) {
  // All elements must be text
  for (final inline in inlines) {
    if (inline.element is! InlineElement_Text) return false;
  }

  // Concatenate all text
  final text = inlines
      .map((e) => (e.element as InlineElement_Text).field0)
      .join();

  // Remove all whitespace
  final trimmed = text.replaceAll(RegExp(r'\s+'), '');
  if (trimmed.isEmpty) return false;

  // Check that the entire trimmed string is only emoji
  final matches = _emojiPattern.allMatches(trimmed).toList();
  final matchedLength = matches.fold<int>(0, (sum, m) => sum + m.end - m.start);
  if (matchedLength != trimmed.length) return false;

  // Count emoji (1-5)
  return matches.isNotEmpty && matches.length <= maxEmojiCount;
}

/// Returns true if the entire message is a jumbo emoji message:
/// single paragraph with only emoji, no attachments.
bool isJumboEmojiMessage(UiMimiContent content) {
  if (content.attachments.isNotEmpty) return false;
  final elements = content.content?.elements;
  if (elements == null || elements.length != 1) return false;
  final block = elements.first.element;
  if (block is! BlockElement_Paragraph) return false;
  return isJumboEmoji(block.field0);
}
