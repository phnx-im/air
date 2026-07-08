// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/foundations/elevation.dart';
import 'package:air/message_list/emoji_repository.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/theme/theme.dart';

// Picker metrics.
const double _emojiCellSize = 52;
const double _emojiGlyphSize = 32;
const double _panelRadius = Spacing.px20;
const double _panelPadding = Spacing.px16;
const double _searchHeight = 40;

/// Border for the search field.
final _pillBorder = OutlineInputBorder(
  borderRadius: BorderRadius.circular(1000),
  borderSide: BorderSide.none,
);

/// Default size of the emoji picker popover.
const Size _emojiPickerPanelSize = Size(360, 360);

/// The emoji picker content: a search field with a skin-tone selector,
/// over a flat scrollable grid of all categorized emojis. Skinnable emojis render with the
/// selected skin tone.
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
    final theme = Theme.of(context);

    final searchController = useTextEditingController();
    final query = useState('');
    final skinTone = useState(initialSkinTone);
    final toneStripOpen = useState(false);

    useEffect(() {
      void listener() => query.value = searchController.text;
      searchController.addListener(listener);
      return () => searchController.removeListener(listener);
    }, [searchController]);

    useEffect(() {
      _EmojiPainters.warmUp(skinTone.value);
      return null;
    }, [skinTone.value]);

    void selectTone(EmojiSkinVariation tone) {
      skinTone.value = tone;
      toneStripOpen.value = false;
      onSkinToneChanged?.call(tone);
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(child: _SearchField(controller: searchController)),
            const SizedBox(width: Spacing.px8),
            _EmojiComponentButton(
              component: skinTone.value,
              onPressed: () => toneStripOpen.value = !toneStripOpen.value,
            ),
          ],
        ),
        if (toneStripOpen.value) ...[
          const SizedBox(height: Spacing.px8),
          _SkinToneStrip(selected: skinTone.value, onSelected: selectTone),
        ],
        const SizedBox(height: Spacing.px12),
        Expanded(
          child: CustomScrollView(
            slivers: [
              for (final (category, emojis) in useMemoized(
                () => EmojiRepository.filter(query.value),
                [query.value],
              )) ...[
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
                    final emoji = emojis[index].applySkinVariation(
                      skinTone.value,
                    );
                    return Padding(
                      padding: const EdgeInsets.all(Spacing.px8),
                      child: _EmojiCell(
                        emoji: emoji,
                        onTap: () => onSelected(emoji),
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

class _EmojiComponentButton extends StatelessWidget {
  const _EmojiComponentButton({
    required this.component,
    required this.onPressed,
  });

  final EmojiSkinVariation component;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return GlassCircleButton(
      size: _searchHeight,
      onPressed: onPressed,
      color: colors.fill.tertiary,
      enableBackdropBlur: false,
      shadows: const [],
      icon: Text(
        '\u{270B}${component.modifier}',
        style: const TextStyle(fontSize: 20, height: 1.0),
      ),
    );
  }
}

class _SkinToneStrip extends StatelessWidget {
  const _SkinToneStrip({required this.selected, required this.onSelected});

  final EmojiSkinVariation selected;
  final ValueChanged<EmojiSkinVariation> onSelected;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    return Row(
      children: [
        for (final tone in EmojiSkinVariation.values)
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
        child: CustomPaint(
          painter: _EmojiGlyphPainter(_EmojiPainters.of(emoji)),
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
    final colors = CustomColorScheme.of(context);
    return Container(
      width: size.width,
      height: size.height,
      padding: const EdgeInsets.all(_panelPadding),
      decoration: BoxDecoration(
        color: colors.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(_panelRadius),
        boxShadow: smallElevationBoxShadows,
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
  Color? barrierColor,
}) {
  return showGeneralDialog<String>(
    context: context,
    barrierDismissible: true,
    barrierColor:
        barrierColor ?? CustomColorScheme.of(context).function.barrier,
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
  Color? barrierColor,
}) {
  return showBottomSheetModal<String>(
    context: context,
    barrierColor: barrierColor,
    builder: (context) => Padding(
      padding: const EdgeInsets.all(_panelPadding),
      child: SizedBox(
        height: _emojiPickerPanelSize.height,
        child: EmojiPicker(
          onSelected: (emoji) => Navigator.of(context).pop(emoji),
          initialSkinTone: initialSkinTone,
          onSkinToneChanged: onSkinToneChanged,
        ),
      ),
    ),
  );
}

/// Paints a cached , pre-laid-out emoji glyph centered in the cell.
class _EmojiGlyphPainter extends CustomPainter {
  const _EmojiGlyphPainter(this.glyph);

  final TextPainter glyph;

  @override
  void paint(Canvas canvas, Size size) {
    glyph.paint(
      canvas,
      Offset((size.width - glyph.width) / 2, (size.height - glyph.height) / 2),
    );
  }

  @override
  bool shouldRepaint(_EmojiGlyphPainter oldDelegate) =>
      glyph != oldDelegate.glyph;
}

/// Process-wide cache of laid-out [TextPainter]s for picker glyphs.
///
/// Cache entries are keyed by the final emoji string (including the skin-tone
/// variant). Entries are kept for the lifetime of the app: measured cost is ~1
/// KiB per glyph, about ~2 MB for the base set and ~4 MB with all skin tones
/// warmed.
class _EmojiPainters {
  static const _warmUpChunkSize = 50;

  static final Map<String, TextPainter> _painters = {};
  static final Set<EmojiSkinVariation> _warmedTones = {};

  /// The painter for [emoji], shaping and caching it on first use.
  static TextPainter of(String emoji) => _painters.putIfAbsent(
    emoji,
    () => TextPainter(
      text: TextSpan(
        text: emoji,
        style: const TextStyle(fontSize: _emojiGlyphSize, height: 1.0),
      ),
      textDirection: TextDirection.ltr,
    )..layout(),
  );

  /// Shapes all picker glyphs for [tone] in idle-priority chunks.
  static void warmUp(EmojiSkinVariation tone) {
    if (!_warmedTones.add(tone)) return;

    final pending = [
      for (final (_, emojis) in EmojiRepository.filter(''))
        for (final emoji in emojis) emoji.applySkinVariation(tone),
    ];

    var index = 0;
    void chunk() {
      final end = math.min(index + _warmUpChunkSize, pending.length);
      for (; index < end; index++) {
        of(pending[index]);
      }
      if (index < pending.length) {
        SchedulerBinding.instance.scheduleTask(chunk, Priority.idle);
      }
    }

    SchedulerBinding.instance.scheduleTask(chunk, Priority.idle);
  }
}
