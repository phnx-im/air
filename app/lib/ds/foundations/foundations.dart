// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The design system foundations: the tokens and primitives every component
/// and screen builds on. Import this instead of the individual token files.
///
/// A token class takes its shape from what varies about it, so a reader can
/// tell what can differ without opening the file. Nothing varies: an
/// `abstract final class` of statics, read where it is used. Varies by tier:
/// an instance per tier and a `current` resolver, passed in as `tokens:`,
/// since the host knows its tier and the component does not. Varies per host:
/// every field defaulted, for values the DS cannot know.
///
/// Inside a tiered class a field is an instance field only where the tiers
/// disagree, so `tokens.x` says x varies and `Tokens.x` says it does not. An
/// invariant field kept on the instance takes a comment arguing for the seam,
/// the way `ListGroupTokens` does. `Button` resolves its own bag: its tier
/// follows the `ButtonSize` its caller already passes.
///
/// A `TextStyle` is resolved from the typescale where it is used, so the style
/// reads beside the text it dresses. Two exceptions: a bag that exists to be
/// host-resolved carries resolved styles, the way `NavItemTokens` does, and a
/// widget whose run has to be measured before it can be laid out exposes its
/// styles as statics, so the measuring pass and the paint cannot disagree, the
/// way `ReactionChip` and `ReactionBar` do.
library;

export 'package:air/ds/foundations/breakpoint.dart';
export 'package:air/ds/foundations/chrome.dart';
export 'package:air/ds/foundations/device_type.dart';
export 'package:air/ds/foundations/dimensions.dart';
export 'package:air/ds/foundations/effects.dart';
export 'package:air/ds/foundations/icons.dart';
export 'package:air/ds/foundations/monospace.dart';
export 'package:air/ds/foundations/primitives.dart';
export 'package:air/ds/foundations/semantic_colors.dart';
export 'package:air/ds/foundations/states.dart';
export 'package:air/ds/foundations/type_scale.dart';
