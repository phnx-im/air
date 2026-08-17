// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/emoji/centered_emoji.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/reaction_emoji_menu/reaction_emoji_menu.dart';
import 'package:air/ds/patterns/reaction_emoji_menu/reaction_emoji_menu_tokens.dart';
import 'package:air/features/emoji/emoji_data.dart' as data;
import 'package:air/features/emoji/emoji_repository.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/haptics.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

// Panel metrics. The menu inside brings its own.
const double _panelRadius = CornerRadius.px20;
const double _panelPadding = S.s16;

/// Default size of the emoji picker popover.
const Size _emojiPickerPanelSize = Size(360, 360);

/// The emoji picker content: the catalog, the query, and the skin tone behind
/// a [ReactionEmojiMenu].
class EmojiPicker extends HookWidget {
  const EmojiPicker({
    super.key,
    required this.onSelected,
    this.initialSkinTone = EmojiSkinVariation.none,
    this.onSkinToneChanged,
  });

  final void Function(String emoji) onSelected;
  final EmojiSkinVariation initialSkinTone;
  final ValueChanged<EmojiSkinVariation>? onSkinToneChanged;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final query = useState('');
    final skinTone = useState(initialSkinTone);

    useEffect(() {
      final tone = skinTone.value;
      // A hook runs its effect while the element is still initialising, where
      // reading an inherited widget is not allowed yet, and the warm-up has to
      // resolve the ambient text style to shape against. So it waits for the
      // frame the picker first paints in.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (context.mounted) _warmUpTone(context, tone);
      });
      return null;
    }, [skinTone.value]);

    // The glyphs carry the tone, so a tone change narrows the catalog again.
    final sections = useMemoized(() => _sections(query.value, skinTone.value), [
      query.value,
      skinTone.value,
    ]);

    return ReactionEmojiMenu(
      tokens: ReactionEmojiMenuTokens.current,
      sections: sections,
      searchHint: loc.emojiPicker_searchHint,
      emptyLabel: loc.emojiPicker_empty,
      tone: EmojiMenuTone(
        options: _toneOptions,
        selected: EmojiSkinVariation.values.indexOf(skinTone.value),
        helpLabel: loc.emojiPicker_skinToneHelp,
        onSelected: (index) {
          final tone = EmojiSkinVariation.values[index];
          skinTone.value = tone;
          onSkinToneChanged?.call(tone);
        },
      ),
      autofocus: true,
      onQueryChanged: (value) => query.value = value,
      onSelected: (emoji) {
        // The menu is a pure view and stays out of the platform, so the feel of
        // a pick is ours to give.
        AppHaptics.selection();
        onSelected(emoji);
      },
    );
  }
}

/// One raised hand per tone: the swatches the tone control offers, in
/// [EmojiSkinVariation.values] order so an index into them names a tone.
final List<String> _toneOptions = [
  for (final variation in EmojiSkinVariation.values)
    '\u{270B}${variation.modifier}',
];

/// The catalog narrowed to [query], every glyph in [tone].
List<EmojiMenuSection> _sections(String query, EmojiSkinVariation tone) => [
  for (final (category, emojis) in EmojiRepository.filter(query))
    EmojiMenuSection(
      title: category,
      emojis: [for (final emoji in emojis) _entry(emoji, tone)],
    ),
];

/// The variants follow [_toneOptions], since the menu hands an index into the
/// swatches back for both the tone and the emoji it picked.
EmojiMenuEntry _entry(data.Emoji emoji, EmojiSkinVariation tone) =>
    EmojiMenuEntry(
      glyph: emoji.applySkinVariation(tone),
      tones: emoji.supportsSkinTone
          ? [
              for (final variation in EmojiSkinVariation.values)
                emoji.applySkinVariation(variation),
            ]
          : const [],
    );

/// A self-contained, fixed-size emoji picker panel for desktop popovers.
class EmojiPickerPanel extends StatelessWidget {
  const EmojiPickerPanel({
    super.key,
    required this.onSelected,
    this.initialSkinTone = EmojiSkinVariation.none,
    this.onSkinToneChanged,
    this.size = _emojiPickerPanelSize,
  });

  final void Function(String emoji) onSelected;
  final EmojiSkinVariation initialSkinTone;
  final ValueChanged<EmojiSkinVariation>? onSkinToneChanged;
  final Size size;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Container(
      width: size.width,
      height: size.height,
      padding: const EdgeInsets.all(_panelPadding),
      decoration: BoxDecoration(
        color: palette.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(_panelRadius),
        boxShadow: Effect.elevation(Elevation.small),
      ),
      child: EmojiPicker(
        onSelected: onSelected,
        initialSkinTone: initialSkinTone,
        onSkinToneChanged: onSkinToneChanged,
      ),
    );
  }
}

/// Shows the emoji picker as a centered popover (desktop) and resolves to the
/// picked emoji, or `null` if dismissed.
Future<String?> showEmojiPickerPopover({
  required BuildContext context,
  EmojiSkinVariation initialSkinTone = EmojiSkinVariation.none,
  ValueChanged<EmojiSkinVariation>? onSkinToneChanged,
}) {
  return showGeneralDialog<String>(
    context: context,
    barrierDismissible: true,
    barrierColor: SemanticPalette.of(context).function.neutral.scrim,
    barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
    transitionDuration: const Duration(milliseconds: 150),
    pageBuilder: (context, animation, secondaryAnimation) =>
        const SizedBox.shrink(),
    transitionBuilder: (dialogContext, animation, secondaryAnimation, child) {
      return FadeTransition(
        opacity: animation,
        child: Center(
          child: Material(
            type: MaterialType.transparency,
            child: EmojiPickerPanel(
              onSelected: (emoji) => Navigator.of(dialogContext).pop(emoji),
              initialSkinTone: initialSkinTone,
              onSkinToneChanged: onSkinToneChanged,
            ),
          ),
        ),
      );
    },
  );
}

/// Shows the emoji picker as a bottom sheet (mobile) and resolves to the picked
/// emoji, or `null` if dismissed.
Future<String?> showEmojiPickerSheet({
  required BuildContext context,
  EmojiSkinVariation initialSkinTone = EmojiSkinVariation.none,
  ValueChanged<EmojiSkinVariation>? onSkinToneChanged,
}) {
  return showAdaptiveModal<String>(
    context: context,
    contentPadding: const EdgeInsets.all(_panelPadding),
    builder: (context) => SizedBox(
      height: _emojiPickerPanelSize.height,
      child: EmojiPicker(
        onSelected: (emoji) => Navigator.of(context).pop(emoji),
        initialSkinTone: initialSkinTone,
        onSkinToneChanged: onSkinToneChanged,
      ),
    ),
  );
}

final Set<EmojiSkinVariation> _warmedTones = {};

/// Shapes all picker glyphs for [tone] via [CenteredEmoji.warmUpGlyphs], once
/// per tone. The style has to be the one the grid paints in, since a painter
/// shaped at another size is a different cache entry.
void _warmUpTone(BuildContext context, EmojiSkinVariation tone) {
  if (!_warmedTones.add(tone)) return;

  CenteredEmoji.warmUpGlyphs(context, [
    for (final (_, emojis) in EmojiRepository.filter(''))
      for (final emoji in emojis) emoji.applySkinVariation(tone),
  ], typeScale.emoji.l.style());
}
