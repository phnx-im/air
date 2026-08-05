// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;

import 'package:air/ds/components/emoji/centered_emoji.dart';
import 'package:air/ds/components/scroll/edge_fade.dart';
import 'package:air/ds/components/scroll/scroll_edges.dart';
import 'package:air/ds/components/searchfield/searchfield.dart';
import 'package:air/ds/components/searchfield/searchfield_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/reaction_emoji_menu/reaction_emoji_menu_tokens.dart';
import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart' show MaterialLocalizations;
import 'package:flutter/services.dart' show TextInputAction;
import 'package:flutter/widgets.dart';

/// One emoji in the grid, in the tone it should read in.
@immutable
class EmojiMenuEntry {
  const EmojiMenuEntry({required this.glyph, this.tones = const []});

  /// The glyph as the grid shows it, the active tone already applied.
  final String glyph;

  /// Every tone this emoji comes in, the default one first, in the same order
  /// as [EmojiMenuTone.options]. Empty for an emoji that takes no tone, which
  /// then has no long-press flyout.
  final List<String> tones;
}

/// A titled run of emoji: a Unicode category, or a grouping of the host's own
/// such as the recently used ones.
@immutable
class EmojiMenuSection {
  const EmojiMenuSection({required this.title, required this.emojis});

  final String title;
  final List<EmojiMenuEntry> emojis;
}

/// The skin-tone control in the header, and the tone the grid is showing.
@immutable
class EmojiMenuTone {
  const EmojiMenuTone({
    required this.options,
    required this.selected,
    required this.helpLabel,
    required this.onSelected,
  });

  /// One sample glyph per tone, the default one first. These are the swatches
  /// the tone picker offers, and [EmojiMenuEntry.tones] follows their order. A
  /// host with none to offer leaves the whole control out rather than passing
  /// an empty list.
  final List<String> options;

  /// Index into [options] of the tone the grid is currently showing, and so of
  /// the sample the header button carries.
  final int selected;

  /// Caption under the swatches, explaining that picking one sets the default.
  final String helpLabel;

  /// Remember [index] as the tone to show emoji in.
  final void Function(int index) onSelected;
}

/// The searchable emoji menu: a search field with a skin-tone control over a
/// scrollable grid of titled sections.
///
/// A pure view. The catalog, the search, and the tone each emoji is shown in
/// are the host's: it hands over the sections to draw, follows the query
/// through [onQueryChanged], and hands back a narrowed list. An empty
/// [sections] is the no-results state.
///
/// Tapping a cell picks it as shown. Long-pressing one that has
/// [EmojiMenuEntry.tones] opens the swatches, which pick that variant and
/// remember its tone for later picks.
///
/// The menu fills the height its host gives it and scrolls inside it, and the
/// surface behind it is the host sheet's or panel's to paint.
class ReactionEmojiMenu extends StatefulWidget {
  const ReactionEmojiMenu({
    super.key,
    required this.tokens,
    required this.sections,
    required this.searchHint,
    required this.emptyLabel,
    this.tone,
    this.autofocus = false,
    this.onQueryChanged,
    this.onSelected,
  });

  final ReactionEmojiMenuTokens tokens;

  /// The grid's content, in the order it should read.
  final List<EmojiMenuSection> sections;

  final String searchHint;

  /// Shown in place of the grid when [sections] comes back empty.
  final String emptyLabel;

  /// The tone control. Null leaves it out of the header, for a host with no
  /// tones to offer.
  final EmojiMenuTone? tone;

  final bool autofocus;

  final ValueChanged<String>? onQueryChanged;

  /// The picked glyph, in the tone it was picked in.
  final ValueChanged<String>? onSelected;

  @override
  State<ReactionEmojiMenu> createState() => _ReactionEmojiMenuState();
}

class _ReactionEmojiMenuState extends State<ReactionEmojiMenu> {
  final _scroll = ScrollController();

  /// How the grid rests against its ends, watched by the two fades. Starts at
  /// rest against both: a short result set never scrolls, and assuming
  /// otherwise flashes a fade on the first frame.
  final _edges = ValueNotifier<ScrollEdges>(
    const ScrollEdges(atTop: true, atBottom: true),
  );

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_onScroll);
    WidgetsBinding.instance.addPostFrameCallback((_) => _onScroll());
  }

  @override
  void dispose() {
    _scroll.removeListener(_onScroll);
    _scroll.dispose();
    _edges.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (!mounted || !_scroll.hasClients) return;
    _edges.value = ScrollEdges.of(_scroll.position);
  }

  @override
  Widget build(BuildContext context) {
    final tokens = widget.tokens;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(height: tokens.headerHeight, child: _header(context)),
        SizedBox(height: tokens.headerGap),
        Expanded(child: _grid(context)),
      ],
    );
  }

  Widget _header(BuildContext context) {
    final tone = widget.tone;
    return Row(
      children: [
        Expanded(
          child: SearchField(
            tokens: SearchFieldTokens.current,
            hintText: widget.searchHint,
            autofocus: widget.autofocus,
            textInputAction: TextInputAction.search,
            onChanged: widget.onQueryChanged,
          ),
        ),
        if (tone != null) ...[
          SizedBox(width: widget.tokens.toneGap),
          _ToneButton(
            tokens: widget.tokens,
            glyph: tone.options[tone.selected],
            onPressed: (anchor) => _openTonePicker(
              anchor: anchor,
              options: tone.options,
              picksEmoji: false,
            ),
          ),
        ],
      ],
    );
  }

  Widget _grid(BuildContext context) {
    final tokens = widget.tokens;
    final palette = SemanticPalette.of(context);

    if (widget.sections.isEmpty) {
      return Center(
        child: Text(
          widget.emptyLabel,
          style: typeScale.body.regular.style(color: palette.text.tertiary),
        ),
      );
    }

    return Stack(
      children: [
        // Metrics notifications cover what a scroll can't: the first layout,
        // and the results changing under a grid that's standing still.
        NotificationListener<ScrollMetricsNotification>(
          onNotification: (notification) {
            _edges.value = ScrollEdges.of(notification.metrics);
            return false;
          },
          child: CustomScrollView(
            controller: _scroll,
            slivers: _sectionSlivers(palette),
          ),
        ),
        Positioned(
          top: 0,
          left: 0,
          right: 0,
          child: _GridFade(
            edges: _edges,
            edge: FadeEdge.top,
            height: tokens.fadeHeight,
          ),
        ),
        Positioned(
          bottom: 0,
          left: 0,
          right: 0,
          child: _GridFade(
            edges: _edges,
            edge: FadeEdge.bottom,
            height: tokens.fadeHeight,
          ),
        ),
      ],
    );
  }

  List<Widget> _sectionSlivers(SemanticPalette palette) {
    final tokens = widget.tokens;
    final delegate = SliverGridDelegateWithMaxCrossAxisExtent(
      maxCrossAxisExtent: tokens.cellExtent,
      mainAxisSpacing: tokens.cellGap,
      crossAxisSpacing: tokens.cellGap,
    );
    // We set it in caps and track it down to a single line, so a title reads
    // as a divider between two runs rather than as a row of its own.
    final titleStyle = typeScale.body.s
        .style(color: palette.text.tertiary, weight: Weight.emphasized)
        .copyWith(height: 1.0);

    return [
      for (final section in widget.sections) ...[
        SliverToBoxAdapter(
          child: Padding(
            padding: EdgeInsets.only(
              top: tokens.sectionTopGap,
              bottom: tokens.sectionBottomGap,
            ),
            child: Text(section.title.toUpperCase(), style: titleStyle),
          ),
        ),
        SliverGrid.builder(
          gridDelegate: delegate,
          itemCount: section.emojis.length,
          itemBuilder: (context, index) => _cell(section.emojis[index]),
        ),
      ],
    ];
  }

  Widget _cell(EmojiMenuEntry entry) {
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () => widget.onSelected?.call(entry.glyph),
        onLongPressStart: entry.tones.isEmpty
            ? null
            : (details) => _openTonePicker(
                anchor: details.globalPosition,
                options: entry.tones,
                picksEmoji: true,
              ),
        child: Center(
          child: CenteredEmoji(emoji: entry.glyph, style: _glyphStyle),
        ),
      ),
    );
  }

  /// Floats the tone swatches over [anchor], a point in global coordinates.
  ///
  /// From a cell they pick the emoji as well as the tone, from the header
  /// button they only set the default, which is what the caption explains.
  void _openTonePicker({
    required Offset anchor,
    required List<String> options,
    required bool picksEmoji,
  }) {
    final tone = widget.tone;
    showGeneralDialog<void>(
      context: context,
      barrierDismissible: true,
      barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
      // No scrim: the barrier is here to catch the dismissing tap, not to dim
      // the menu the swatches came out of.
      barrierColor: const Color(0x00000000),
      transitionDuration: Effect.duration(ReactionEmojiMenuTokens.flyoutEnter),
      transitionBuilder: (context, animation, _, child) => FadeTransition(
        opacity: animation.drive(CurveTween(curve: Effect.easeOutQuart)),
        child: child,
      ),
      pageBuilder: (dialogContext, _, _) => _TonePickerPage(
        tokens: widget.tokens,
        anchor: anchor,
        options: options,
        selected: tone?.selected ?? -1,
        helpLabel: picksEmoji ? null : tone?.helpLabel,
        onPicked: (index) {
          Navigator.of(dialogContext).pop();
          tone?.onSelected(index);
          if (picksEmoji) widget.onSelected?.call(options[index]);
        },
      ),
    );
  }
}

/// Grid and swatch glyphs stand on their own rather than sitting alongside
/// text, so they take the large inline emoji step.
TextStyle get _glyphStyle => typeScale.emoji.l.style();

/// The header's glyph sits in a button the size of the search field beside it,
/// so it stays at reading size instead of taking the emoji scale.
TextStyle get _toneGlyphStyle => typeScale.body.l.style().copyWith(height: 1.0);

/// The round button carrying the active tone, which opens the swatches.
class _ToneButton extends StatelessWidget {
  const _ToneButton({
    required this.tokens,
    required this.glyph,
    required this.onPressed,
  });

  final ReactionEmojiMenuTokens tokens;
  final String glyph;

  /// Reports the button's own position, so the swatches open out of it.
  final void Function(Offset anchor) onPressed;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return StateLayer(
      borderRadius: ReactionEmojiMenuTokens.toneButtonRadius,
      surface: palette.fill.tertiary,
      background: DecoratedBox(
        decoration: BoxDecoration(
          color: palette.fill.tertiary,
          shape: BoxShape.circle,
        ),
      ),
      onTap: () {
        final box = context.findRenderObject();
        onPressed(
          box is RenderBox ? box.localToGlobal(Offset.zero) : Offset.zero,
        );
      },
      child: SizedBox.square(
        dimension: tokens.headerHeight,
        child: Center(
          child: CenteredEmoji(emoji: glyph, style: _toneGlyphStyle),
        ),
      ),
    );
  }
}

/// Fade over one end of the grid, painted only while there's a row beyond it.
/// It masks against the elevated surface the host paints, so a row dissolves
/// into it rather than getting cut off at the edge.
class _GridFade extends StatelessWidget {
  const _GridFade({
    required this.edges,
    required this.edge,
    required this.height,
  });

  final ValueListenable<ScrollEdges> edges;
  final FadeEdge edge;
  final double height;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<ScrollEdges>(
      valueListenable: edges,
      child: EdgeFade(
        edge: edge,
        height: height,
        color: SemanticPalette.of(context).backgroundElevated.primary,
        curve: Curves.easeInOutQuad,
      ),
      builder: (context, value, child) {
        final atRest = switch (edge) {
          FadeEdge.top => value.atTop,
          FadeEdge.bottom => value.atBottom,
        };
        return AnimatedOpacity(
          opacity: atRest ? 0 : 1,
          duration: Effect.duration(MotionPreset.short),
          curve: Effect.easeOutQuart,
          child: child,
        );
      },
    );
  }
}

/// Places the tone card over the point the swatches were asked for.
class _TonePickerPage extends StatelessWidget {
  const _TonePickerPage({
    required this.tokens,
    required this.anchor,
    required this.options,
    required this.selected,
    required this.helpLabel,
    required this.onPicked,
  });

  final ReactionEmojiMenuTokens tokens;

  /// The point the swatches were asked for, in global coordinates.
  final Offset anchor;

  final List<String> options;

  /// Index of the swatch to mark as the active tone. Out of range marks none,
  /// which is where a host offers variants but keeps no default.
  final int selected;

  /// Caption under the swatches. Null in the per-emoji flyout, which picks
  /// rather than explains.
  final String? helpLabel;

  final void Function(int index) onPicked;

  @override
  Widget build(BuildContext context) {
    final local = _toOverlaySpace(context, anchor);
    final ceiling =
        MediaQuery.viewPaddingOf(context).top +
        ReactionEmojiMenuTokens.flyoutEdgeInset;
    final top = math.max(
      ceiling,
      local.dy -
          (_glyphStyle.fontSize ?? 0) -
          ReactionEmojiMenuTokens.flyoutAnchorGap,
    );

    // Only the vertical position tracks the anchor. A full-width slot plus a
    // Center keeps the card on the viewport's axis at any size, without having
    // to guess how wide it came out.
    return Stack(
      children: [
        Positioned(
          top: top,
          left: 0,
          right: 0,
          child: Center(
            child: _ToneCard(
              tokens: tokens,
              options: options,
              selected: selected,
              helpLabel: helpLabel,
              onPicked: onPicked,
            ),
          ),
        ),
      ],
    );
  }

  /// The anchor arrives in global coordinates while the card lays out inside
  /// the overlay, which the app's interface scale can move and scale.
  Offset _toOverlaySpace(BuildContext context, Offset point) {
    final overlay = Overlay.of(context).context.findRenderObject();
    if (overlay is! RenderBox || !overlay.hasSize) return point;
    return overlay.globalToLocal(point);
  }
}

/// The floating card: one swatch per tone, over an optional caption.
class _ToneCard extends StatelessWidget {
  const _ToneCard({
    required this.tokens,
    required this.options,
    required this.selected,
    required this.helpLabel,
    required this.onPicked,
  });

  final ReactionEmojiMenuTokens tokens;
  final List<String> options;
  final int selected;
  final String? helpLabel;
  final void Function(int index) onPicked;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final helpLabel = this.helpLabel;

    return Container(
      padding: tokens.flyoutPadding,
      decoration: BoxDecoration(
        color: palette.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(
          ReactionEmojiMenuTokens.flyoutRadius,
        ),
        boxShadow: Effect.elevation(Elevation.medium),
      ),
      child: DefaultTextStyle.merge(
        style: typeScale.body.s.style(color: palette.text.primary),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (var i = 0; i < options.length; i++) ...[
                  if (i > 0) SizedBox(width: tokens.flyoutItemGap),
                  _ToneSwatch(
                    tokens: tokens,
                    glyph: options[i],
                    selected: i == selected,
                    onTap: () => onPicked(i),
                  ),
                ],
              ],
            ),
            if (helpLabel != null) ...[
              SizedBox(height: tokens.flyoutHelpGap),
              Text(
                helpLabel,
                style: typeScale.body.s
                    .style(color: palette.text.tertiary)
                    .copyWith(height: 1.0),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

/// One tone in the card. The active one carries a fill, the way the card says
/// which tone the grid is already in.
class _ToneSwatch extends StatelessWidget {
  const _ToneSwatch({
    required this.tokens,
    required this.glyph,
    required this.selected,
    required this.onTap,
  });

  final ReactionEmojiMenuTokens tokens;
  final String glyph;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return StateLayer(
      borderRadius: ReactionEmojiMenuTokens.toneButtonRadius,
      surface: selected
          ? palette.fill.tertiary
          : palette.backgroundElevated.primary,
      selected: selected,
      background: selected
          ? DecoratedBox(
              decoration: BoxDecoration(
                color: palette.fill.tertiary,
                shape: BoxShape.circle,
              ),
            )
          : null,
      onTap: onTap,
      child: Padding(
        padding: tokens.flyoutItemPadding,
        child: CenteredEmoji(emoji: glyph, style: _glyphStyle),
      ),
    );
  }
}
