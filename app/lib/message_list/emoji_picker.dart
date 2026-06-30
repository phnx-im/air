// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/message_list/emoji_data_generated.dart' as emoji_data;
import 'package:air/message_list/emoji_repository.dart';
import 'package:flutter/material.dart';

import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/theme/theme.dart';

/// Applies [tone] to [entry] using its precomputed skin-tone variant, falling
/// back to the base emoji when the tone is [EmojiSkinTone.none] or the variant
/// is missing. Using the variant table (rather than appending the modifier)
/// keeps ZWJ and multi-code-point emojis correct.
String applySkinTone(emoji_data.Emoji entry, EmojiSkinTone tone) {
  if (tone == EmojiSkinTone.none) {
    return entry.emoji;
  }
  return entry.skinVariations[tone.modifier] ?? entry.emoji;
}

// Picker metrics.
const double _emojiCellSize = 52;
const double _emojiGlyphSize = 32;
const double _panelRadius = Spacing.px20;
const double _panelPadding = Spacing.px16;
const double _searchHeight = 40;

/// Stadium-shaped border for the search field. The oversized radius is clamped
/// to half the painted height, yielding fully rounded (semicircular) ends.
final _pillBorder = OutlineInputBorder(
  borderRadius: BorderRadius.circular(1000),
  borderSide: BorderSide.none,
);

/// Default size of the desktop emoji-picker popover.
const Size emojiPickerPanelSize = Size(360, 360);

/// The emoji picker content: a pinned search field with a skin-tone selector,
/// over a flat scrollable grid of all emojis. Skinnable emojis render with the
/// selected skin tone.
///
/// Picking an emoji calls [onSelected] with the (tone-applied) emoji. The
/// skin-tone selector starts at [initialSkinTone] and reports changes through
/// [onSkinToneChanged].
class EmojiPicker extends StatefulWidget {
  const EmojiPicker({
    super.key,
    required this.onSelected,
    this.initialSkinTone = EmojiSkinTone.none,
    this.onSkinToneChanged,
  });

  final void Function(String emoji) onSelected;
  final EmojiSkinTone initialSkinTone;
  final ValueChanged<EmojiSkinTone>? onSkinToneChanged;

  @override
  State<EmojiPicker> createState() => _EmojiPickerState();
}

class _EmojiPickerState extends State<EmojiPicker> {
  final TextEditingController _searchController = TextEditingController();
  String _query = '';
  late EmojiSkinTone _skinTone = widget.initialSkinTone;
  bool _toneStripOpen = false;

  @override
  void initState() {
    super.initState();
    _searchController.addListener(() {
      if (_searchController.text != _query) {
        setState(() => _query = _searchController.text);
      }
    });
  }

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  void _selectTone(EmojiSkinTone tone) {
    setState(() {
      _skinTone = tone;
      _toneStripOpen = false;
    });
    widget.onSkinToneChanged?.call(tone);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(child: _SearchField(controller: _searchController)),
            const SizedBox(width: Spacing.px8),
            _SkinToneButton(
              tone: _skinTone,
              onPressed: () => setState(() => _toneStripOpen = !_toneStripOpen),
            ),
          ],
        ),
        if (_toneStripOpen) ...[
          const SizedBox(height: Spacing.px8),
          _SkinToneStrip(selected: _skinTone, onSelected: _selectTone),
        ],
        const SizedBox(height: Spacing.px12),
        Expanded(
          child: CustomScrollView(
            slivers: [
              for (final (category, emojis) in emoji_data.emojisByCategory) ...[
                SliverToBoxAdapter(
                  child: Padding(
                    padding: const EdgeInsets.all(Spacing.px8),
                    child: Text(category, style: theme.textTheme.bodySmall),
                  ),
                ),
                SliverGrid.builder(
                  gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
                    maxCrossAxisExtent: _emojiCellSize,
                    mainAxisExtent: _emojiCellSize,
                  ),
                  itemCount: emojis.length,
                  itemBuilder: (context, index) {
                    final emoji = applySkinTone(emojis[index], _skinTone);
                    return Padding(
                      padding: const EdgeInsets.all(Spacing.px8),
                      child: _EmojiCell(
                        emoji: emoji,
                        onTap: () => widget.onSelected(emoji),
                      ),
                    );
                  },
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}

class _SearchField extends StatelessWidget {
  const _SearchField({required this.controller});

  final TextEditingController controller;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return SizedBox(
      height: _searchHeight,
      child: TextField(
        controller: controller,
        textInputAction: TextInputAction.search,
        style: TextStyle(
          fontSize: FontSizes.base.size,
          color: colors.text.primary,
        ),
        decoration: InputDecoration(
          filled: true,
          fillColor: colors.fill.tertiary,
          hintText: 'Search emoji',
          hintStyle: TextStyle(
            fontSize: FontSizes.base.size,
            color: colors.text.tertiary,
          ),
          prefixIcon: Padding(
            padding: const EdgeInsets.only(
              left: Spacing.px12,
              right: Spacing.px8,
            ),
            child: AppIcon.search(size: 18, color: colors.text.tertiary),
          ),
          prefixIconConstraints: const BoxConstraints(
            minWidth: 0,
            minHeight: 0,
          ),
          contentPadding: const EdgeInsets.symmetric(horizontal: Spacing.px12),
          // Override the theme's enabled/focused borders (radius 8) so the
          // field is a full pill regardless of its painted height.
          border: _pillBorder,
          enabledBorder: _pillBorder,
          focusedBorder: _pillBorder,
        ),
      ),
    );
  }
}

class _SkinToneButton extends StatelessWidget {
  const _SkinToneButton({required this.tone, required this.onPressed});

  final EmojiSkinTone tone;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return GlassCircleButton(
      size: _searchHeight,
      onPressed: onPressed,
      color: colors.fill.tertiary,
      enableBackdropBlur: false,
      icon: Text(
        '\u{270B}${tone.modifier}',
        style: const TextStyle(fontSize: 20, height: 1.0),
      ),
    );
  }
}

class _SkinToneStrip extends StatelessWidget {
  const _SkinToneStrip({required this.selected, required this.onSelected});

  final EmojiSkinTone selected;
  final ValueChanged<EmojiSkinTone> onSelected;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Row(
      children: [
        for (final tone in EmojiSkinTone.values)
          Expanded(
            child: Center(
              child: GlassCircleButton(
                size: _emojiCellSize,
                onPressed: () => onSelected(tone),
                enableBackdropBlur: false,
                shadows: const [],
                color: tone == selected
                    ? colors.backgroundBase.secondary
                    : Colors.transparent,
                icon: Text(
                  '\u{270B}${tone.modifier}',
                  style: const TextStyle(
                    fontSize: _emojiGlyphSize,
                    height: 1.0,
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

class _EmojiCell extends StatelessWidget {
  const _EmojiCell({required this.emoji, required this.onTap});

  final String emoji;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Center(
          child: Text(
            emoji,
            style: const TextStyle(fontSize: _emojiGlyphSize, height: 1.0),
          ),
        ),
      ),
    );
  }
}

/// A self-contained, fixed-size emoji picker panel for desktop popovers.
class EmojiPickerPanel extends StatelessWidget {
  const EmojiPickerPanel({
    super.key,
    required this.onSelected,
    this.initialSkinTone = EmojiSkinTone.none,
    this.onSkinToneChanged,
    this.size = emojiPickerPanelSize,
  });

  final void Function(String emoji) onSelected;
  final EmojiSkinTone initialSkinTone;
  final ValueChanged<EmojiSkinTone>? onSkinToneChanged;
  final Size size;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Container(
      width: size.width,
      height: size.height,
      padding: const EdgeInsets.all(_panelPadding),
      decoration: BoxDecoration(
        color: colors.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(_panelRadius),
        boxShadow: const [
          BoxShadow(
            color: Color(0x33000000),
            blurRadius: 24,
            offset: Offset(0, 8),
          ),
        ],
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
  EmojiSkinTone initialSkinTone = EmojiSkinTone.none,
  ValueChanged<EmojiSkinTone>? onSkinToneChanged,
}) {
  return showGeneralDialog<String>(
    context: context,
    barrierDismissible: true,
    barrierColor: const Color(0x33000000),
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
  EmojiSkinTone initialSkinTone = EmojiSkinTone.none,
  ValueChanged<EmojiSkinTone>? onSkinToneChanged,
}) {
  return showBottomSheetModal<String>(
    context: context,
    builder: (context) => Padding(
      padding: const EdgeInsets.all(_panelPadding),
      child: SizedBox(
        height: emojiPickerPanelSize.height,
        child: EmojiPicker(
          onSelected: (emoji) => Navigator.of(context).pop(emoji),
          initialSkinTone: initialSkinTone,
          onSkinToneChanged: onSkinToneChanged,
        ),
      ),
    ),
  );
}
