// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

import 'package:air/ds/components/button/glass_circle_button.dart';
import 'package:air/ds/components/modal/bottom_sheet_modal.dart';
import 'package:air/ds/foundations/font_size.dart';
import 'package:air/ds/foundations/icons/app_icons.dart';
import 'package:air/ds/foundations/themes.dart';
import 'package:air/ds/theme/theme.dart';

import 'emoji_repository.dart';

/// Unicode skin-tone modifiers. [EmojiSkinTone.none] is the default yellow.
enum EmojiSkinTone {
  none(''),
  light('\u{1F3FB}'),
  mediumLight('\u{1F3FC}'),
  medium('\u{1F3FD}'),
  mediumDark('\u{1F3FE}'),
  dark('\u{1F3FF}');

  const EmojiSkinTone(this.modifier);

  /// The Unicode skin-tone modifier appended to a skinnable base emoji.
  final String modifier;
}

/// Applies [tone] to [entry] when the emoji supports skin tones, otherwise
/// returns the base emoji unchanged.
String applySkinTone(EmojiEntry entry, EmojiSkinTone tone) {
  if (!entry.supportsSkinTone || tone == EmojiSkinTone.none) {
    return entry.emoji;
  }
  return '${entry.emoji}${tone.modifier}';
}

// Picker metrics.
const double emojiCellSize = 40;
const double _emojiGlyphSize = 24;
const double _panelRadius = Spacing.px20;
const double _panelPadding = Spacing.px16;
const double _searchHeight = 40;

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
  // Loaded once and reused across picker openings.
  static Future<EmojiRepository>? _repositoryFuture;

  final TextEditingController _searchController = TextEditingController();
  String _query = '';
  late EmojiSkinTone _skinTone = widget.initialSkinTone;
  bool _toneStripOpen = false;

  @override
  void initState() {
    super.initState();
    _repositoryFuture ??= EmojiRepository.load();
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
          child: FutureBuilder<EmojiRepository>(
            future: _repositoryFuture,
            builder: (context, snapshot) {
              final repository = snapshot.data;
              if (repository == null) {
                return const Center(child: CircularProgressIndicator());
              }
              final entries = repository.filter(_query);
              return GridView.builder(
                padding: EdgeInsets.zero,
                gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
                  maxCrossAxisExtent: emojiCellSize,
                  mainAxisExtent: emojiCellSize,
                ),
                itemCount: entries.length,
                itemBuilder: (context, index) {
                  final emoji = applySkinTone(entries[index], _skinTone);
                  return _EmojiCell(
                    emoji: emoji,
                    onTap: () => widget.onSelected(emoji),
                  );
                },
              );
            },
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
          fillColor: colors.backgroundBase.secondary,
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
          border: OutlineInputBorder(
            borderRadius: BorderRadius.circular(_searchHeight / 2),
            borderSide: BorderSide.none,
          ),
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
    return GlassCircleButton(
      size: _searchHeight,
      onPressed: onPressed,
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
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onTap: () => onSelected(tone),
              child: Container(
                height: emojiCellSize,
                margin: const EdgeInsets.symmetric(horizontal: Spacing.px4 / 2),
                decoration: BoxDecoration(
                  color: tone == selected
                      ? colors.backgroundBase.secondary
                      : Colors.transparent,
                  borderRadius: BorderRadius.circular(Spacing.px8),
                ),
                child: Center(
                  child: Text(
                    '\u{270B}${tone.modifier}',
                    style: const TextStyle(
                      fontSize: _emojiGlyphSize,
                      height: 1.0,
                    ),
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
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: Center(
        child: Text(
          emoji,
          style: const TextStyle(fontSize: _emojiGlyphSize, height: 1.0),
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
