// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The design system foundations: the tokens and primitives every component
/// and screen builds on. Import this instead of the individual token files.
///
/// Token classes with tiered instances (per density or per device) follow one
/// shape rule: a field lives on the instance only when its value differs
/// between the tiers, and anything with one value across all tiers is a
/// static const. So `tokens.x` says x varies with the tier and `Tokens.x`
/// says it does not. Keeping an invariant field on the instance takes a
/// comment arguing for the seam, the way ListGroupTokens does. Single-tier
/// classes keep instance fields: there the bag is the delivery vehicle, not
/// a variance claim.
library;

export 'package:air/ds/foundations/breakpoint.dart';
export 'package:air/ds/foundations/device_type.dart';
export 'package:air/ds/foundations/dimensions.dart';
export 'package:air/ds/foundations/effects.dart';
export 'package:air/ds/foundations/icons.dart';
export 'package:air/ds/foundations/monospace.dart';
export 'package:air/ds/foundations/primitives.dart';
export 'package:air/ds/foundations/semantic_colors.dart';
export 'package:air/ds/foundations/states.dart';
export 'package:air/ds/foundations/type_scale.dart';
