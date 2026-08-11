// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/text_input/text_input.dart';
import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_guard.dart';
import 'package:air/ds/patterns/modal/modal_stack.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/util/username_input_formatter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:image_picker/image_picker.dart';

/// Exact length of an invitation code.
const int _codeLength = 8;

/// The code alphabet, leaving out glyphs that read as one another.
const String _codeAlphabet = r'[A-HJKMNP-Z0-9]';

/// What the flow asks for, in order.
enum _Step {
  invitationCode,
  profile,

  /// Reached only once the account exists, so there is no going back from it.
  username,
}

/// Creating an account: the invitation code, then the profile, then a username.
///
/// One flow rather than three screens: the account is created between the last
/// two steps, so a screen stack would have a rung nobody could return to. The
/// flow also decides when it is done.
class AccountCreationFlow extends StatefulWidget {
  const AccountCreationFlow({super.key});

  @override
  State<AccountCreationFlow> createState() => _AccountCreationFlowState();
}

class _AccountCreationFlowState extends State<AccountCreationFlow> {
  _Step _step = _Step.invitationCode;

  /// Errors only show once a step has been submitted.
  bool _showErrors = false;

  /// Revealed by a long press on a step's copy, kept across steps. Behind the
  /// experiments switch, pointing the client at another backend is not ready
  /// yet.
  bool _serverFieldVisible = false;

  bool _isAddingUsername = false;

  /// Whether the server rejected the username in the field. Only true of the
  /// exact text sent, so the next edit clears it.
  bool _usernameTaken = false;

  String? _codeError;
  String? _displayNameError;
  String? _domainError;
  String? _usernameError;

  late final TextEditingController _codeController;
  late final TextEditingController _displayNameController;
  late final TextEditingController _domainController;
  late final TextEditingController _usernameController;

  @override
  void initState() {
    super.initState();
    // The fields open on what the cubit already holds rather than on blanks.
    final registration = context.read<RegistrationCubit>().state;
    _codeController = TextEditingController(
      text: registration.invitationCode ?? '',
    );
    _displayNameController = TextEditingController(
      text: registration.displayName,
    );
    _domainController = TextEditingController(text: registration.domain);
    _usernameController = TextEditingController();
  }

  @override
  void dispose() {
    _codeController.dispose();
    _displayNameController.dispose();
    _domainController.dispose();
    _usernameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return PopScope(
      // Only guards the busy state, pinning the route while the server works.
      canPop: !_isBusy,
      child: ModalPageStack(
        onBack: _back,
        onDismiss: _dismiss,
        pages: [
          ModalStackEntry(
            key: const ValueKey('account-creation-code'),
            child: _pane(loc, _Step.invitationCode),
          ),
          if (_step != _Step.invitationCode)
            ModalStackEntry(
              key: const ValueKey('account-creation-profile'),
              child: _pane(loc, _Step.profile),
            ),
          if (_step == _Step.username)
            // The account exists behind this step, so neither action leads
            // anywhere it could go.
            ModalStackEntry(
              key: const ValueKey('account-creation-username'),
              canGoBack: false,
              canDismiss: false,
              child: _pane(loc, _Step.username),
            ),
        ],
      ),
    );
  }

  // Chrome

  /// One step as a page of the stack. The pattern derives the header's leading
  /// action from the level the page sits at, so only the skip is placed here.
  Widget _pane(AppLocalizations loc, _Step step) {
    return ModalPane(
      title: switch (step) {
        _Step.invitationCode => loc.invitationCodeScreen_header,
        _Step.profile => loc.signUpScreen_header,
        _Step.username => loc.usernameOnboarding_header,
      },
      // The last step offers a skip instead of a way back.
      trailing: step == _Step.username
          ? Button(
              size: ButtonSize.small,
              type: ButtonType.secondary,
              label: loc.usernameOnboarding_next,
              state: _isAddingUsername
                  ? ButtonState.disabled
                  : ButtonState.active,
              onPressed: _finish,
            )
          : null,
      footer: _footer(loc, step),
      // The fields belong to the flow rather than to the step showing them, so
      // what any of them holds is what leaving the flow would drop.
      child: ModalDismissGuard(
        hasUnsavedInput: _hasEnteredData,
        child: _body(loc, step),
      ),
    );
  }

  /// The domain is left out: it comes pre-filled from the server the client
  /// points at, so counting it would make every step look half-written.
  bool _hasEnteredData() =>
      _codeController.text.trim().isNotEmpty ||
      _displayNameController.text.trim().isNotEmpty ||
      _usernameController.text.trim().isNotEmpty;

  Widget _footer(AppLocalizations loc, _Step step) {
    final registration = context.watch<RegistrationCubit>().state;
    final (label, onPressed, isBusy) = switch (step) {
      _Step.invitationCode => (
        loc.invitationCodeScreen_actionButton,
        _submitCode,
        registration.isCheckingInvitationCode,
      ),
      _Step.profile => (
        loc.signUpScreen_actionButton,
        _submitProfile,
        registration.isSigningUp,
      ),
      _Step.username => (
        loc.usernameOnboarding_addButton,
        _submitUsername,
        _isAddingUsername,
      ),
    };

    return Button(
      label: label,
      onPressed: onPressed,
      state: isBusy ? ButtonState.pending : ButtonState.active,
    );
  }

  Widget _body(AppLocalizations loc, _Step step) {
    return GestureDetector(
      onTap: () => FocusScope.of(context).unfocus(),
      behavior: HitTestBehavior.translucent,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          ModalShellTokens.contentPaddingLeft,
          S.s12,
          ModalShellTokens.contentPaddingRight,
          S.s12,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: switch (step) {
            _Step.invitationCode => _invitationCodeStep(loc),
            _Step.profile => _profileStep(loc),
            _Step.username => _usernameStep(loc),
          },
        ),
      ),
    );
  }

  // Steps

  List<Widget> _invitationCodeStep(AppLocalizations loc) {
    return [
      _Copy(
        text: loc.invitationCodeScreen_subheader,
        onLongPress: _revealServerField,
      ),
      const SizedBox(height: S.s32),
      AppTextInput(
        tokens: AppTextInputTokens.current,
        controller: _codeController,
        autofocus: true,
        label: loc.invitationCodeScreen_inputLabel,
        hintText: loc.invitationCodeScreen_inputHint,
        // The field caps the length silently, so the counter reports progress.
        // An error takes the same line.
        helperText: '${_codeController.text.length}/$_codeLength',
        errorText: _codeError,
        maxLength: _codeLength,
        inputFormatters: [
          // A hardware keyboard ignores the shift hint, so input is uppercased
          // before the filter below applies.
          TextInputFormatter.withFunction(
            (oldValue, newValue) =>
                newValue.copyWith(text: newValue.text.toUpperCase()),
          ),
          FilteringTextInputFormatter.allow(RegExp(_codeAlphabet)),
        ],
        textCapitalization: TextCapitalization.characters,
        keyboardType: TextInputType.visiblePassword,
        onChanged: (value) {
          context.read<RegistrationCubit>().setInvitationCode(value);
          // The counter reads the controller, so every keystroke must rebuild.
          setState(_revalidate);
        },
        onSubmitted: (_) => _submitCode(),
      ),
      ..._serverField(loc),
    ];
  }

  List<Widget> _profileStep(AppLocalizations loc) {
    return [
      _Copy(text: loc.signUpScreen_subheader, onLongPress: _revealServerField),
      const SizedBox(height: S.s32),
      Align(
        alignment: Alignment.center,
        child: GestureDetector(
          onTap: _pickAvatar,
          onLongPress: _revealServerField,
          child: const _AvatarPicker(),
        ),
      ),
      const SizedBox(height: S.s32),
      AppTextInput(
        tokens: AppTextInputTokens.current,
        controller: _displayNameController,
        autofocus: true,
        label: loc.signUpScreen_displayNameInputName,
        hintText: loc.signUpScreen_displayNameInputHint,
        errorText: _displayNameError,
        onChanged: (value) {
          context.read<RegistrationCubit>().setDisplayName(value);
          setState(_revalidate);
        },
        onSubmitted: (_) => _submitProfile(),
      ),
      ..._serverField(loc),
    ];
  }

  List<Widget> _usernameStep(AppLocalizations loc) {
    return [
      _Copy(text: loc.usernameOnboarding_body),
      const SizedBox(height: S.s24),
      AppTextInput(
        tokens: AppTextInputTokens.current,
        controller: _usernameController,
        autofocus: true,
        textInputAction: TextInputAction.done,
        label: loc.usernameOnboarding_usernameInputName,
        hintText: loc.usernameOnboarding_usernameInputHint,
        helperText: loc.usernameOnboarding_syntax,
        errorText: _usernameError,
        inputFormatters: const [UsernameInputFormatter()],
        onChanged: (value) {
          // Taken only applies to the exact text sent, so the next edit reruns
          // the local rules.
          if (!_usernameTaken) return;
          setState(() {
            _usernameTaken = false;
            _usernameError = _validateUsername(loc, value);
          });
        },
        onSubmitted: (_) => _submitUsername(),
      ),
    ];
  }

  /// The server field, on the steps that carry it and once it is revealed.
  List<Widget> _serverField(AppLocalizations loc) {
    if (!_serverFieldVisible) return const [];
    return [
      const SizedBox(height: S.s24),
      Text(
        loc.signUpScreen_serverLabel,
        style: Theme.of(context).textTheme.bodyMedium,
      ),
      const SizedBox(height: S.s16),
      AppTextInput(
        tokens: AppTextInputTokens.current,
        controller: _domainController,
        hintText: loc.signUpScreen_serverHint,
        errorText: _domainError,
        onChanged: (value) {
          context.read<RegistrationCubit>().setDomain(value);
          setState(_revalidate);
        },
        onSubmitted: (_) => switch (_step) {
          _Step.invitationCode => _submitCode(),
          _Step.profile => _submitProfile(),
          _Step.username => null,
        },
      ),
    ];
  }

  // Navigation between the steps

  /// Whether the flow is waiting on the server.
  bool get _isBusy {
    final registration = context.read<RegistrationCubit>().state;
    return registration.isCheckingInvitationCode ||
        registration.isSigningUp ||
        _isAddingUsername;
  }

  void _back() {
    if (_isBusy) return;
    switch (_step) {
      case _Step.invitationCode:
        break;
      case _Step.profile:
        _goTo(_Step.invitationCode);
      // The account already exists, so there is nothing to go back to.
      case _Step.username:
        break;
    }
  }

  /// Leaves the flow without an account.
  void _dismiss() {
    if (_isBusy) return;
    context.read<NavigationCubit>().pop();
  }

  void _goTo(_Step step) {
    FocusScope.of(context).unfocus();
    setState(() {
      _step = step;
      _showErrors = false;
      _codeError = null;
      _displayNameError = null;
      _domainError = null;
      _usernameError = null;
    });
  }

  /// Leaves the flow for the app, whether or not a username was added.
  void _finish() => context.read<NavigationCubit>().openHome();

  void _revealServerField() {
    final settings = context.read<UserSettingsCubit>().state;
    if (!settings.experimentalFeaturesActive) {
      return;
    }
    setState(() => _serverFieldVisible = true);
  }

  // Validation

  /// Re-runs the step's rules once its errors are showing.
  void _revalidate() {
    if (_showErrors) _validateStep();
  }

  /// Puts the step's errors on screen and reports whether it may be submitted.
  bool _submitAllowed() {
    var allowed = false;
    setState(() {
      _showErrors = true;
      allowed = _validateStep();
    });
    return allowed;
  }

  /// Recomputes the step's errors into the fields. The rebuild is the caller's.
  bool _validateStep() {
    final loc = AppLocalizations.of(context);
    final registration = context.read<RegistrationCubit>().state;

    _codeError =
        _step == _Step.invitationCode &&
            (registration.invitationCode?.length ?? 0) != _codeLength
        ? loc.invitationCodeScreen_error_invalidLength
        : null;
    _displayNameError =
        _step == _Step.profile && registration.displayName.trim().isEmpty
        ? loc.signUpScreen_error_emptyDisplayName
        : null;
    // The rule follows the value, not the field: the account is created
    // against the domain whether or not it is on screen. The username step is
    // past that point and carries no rule.
    _domainError = _step != _Step.username && !registration.isDomainValid
        ? loc.signUpScreen_error_invalidDomain
        : null;

    return _codeError == null &&
        _displayNameError == null &&
        _domainError == null;
  }

  String? _validateUsername(AppLocalizations loc, String value) {
    if (_usernameTaken) return loc.usernameScreen_error_alreadyExists;
    final normalized = UsernameInputFormatter.normalize(value);
    if (normalized.isEmpty) return loc.usernameScreen_error_emptyUsername;
    return switch (UiUsername(plaintext: normalized).validationError()) {
      UsernameValidationError.tooShort => loc.usernameScreen_error_tooShort,
      UsernameValidationError.tooLong => loc.usernameScreen_error_tooLong,
      UsernameValidationError.invalidCharacter =>
        loc.usernameScreen_error_invalidCharacter,
      UsernameValidationError.consecutiveDashes =>
        loc.usernameScreen_error_consecutiveDashes,
      UsernameValidationError.leadingDigit =>
        loc.usernameScreen_error_leadingDigit,
      null => null,
    };
  }

  // Submits

  Future<void> _submitCode() async {
    if (!_submitAllowed()) return;

    final registrationCubit = context.read<RegistrationCubit>();
    final error = await registrationCubit.submitInvitationCode();
    if (!mounted) return;

    if (error == null) {
      _goTo(_Step.profile);
      return;
    }
    showErrorBannerStandalone(
      (loc) => switch (error.code) {
        .missing => loc.invitationCodeScreen_error_missing,
        .invalid => loc.invitationCodeScreen_error_invalid,
        .internal => loc.invitationCodeScreen_error_internal(
          error.message ?? "",
        ),
      },
    );
  }

  Future<void> _submitProfile() async {
    if (!_submitAllowed()) return;

    final registrationCubit = context.read<RegistrationCubit>();
    final displayName = registrationCubit.state.displayName;
    final error = await registrationCubit.signUp();
    if (!mounted) return;

    if (error != null) {
      showErrorBannerStandalone(
        (loc) => loc.signUpScreen_error_register(error.message),
      );
      return;
    }

    _usernameController.text = _usernameSuggestion(displayName);
    _goTo(_Step.username);
  }

  Future<void> _submitUsername() async {
    if (_isAddingUsername) return;
    final loc = AppLocalizations.of(context);

    final error = _validateUsername(loc, _usernameController.text);
    if (error != null) {
      setState(() => _usernameError = error);
      return;
    }

    final username = UiUsername(
      plaintext: UsernameInputFormatter.normalize(
        _usernameController.text.trim(),
      ),
    );
    setState(() {
      _usernameTaken = false;
      _isAddingUsername = true;
    });

    // The privacy pass tokens may not have arrived yet for the fresh account,
    // so a failure is worth one retry.
    if (!await _addUsername(username, reportFailure: false)) {
      await Future.delayed(const Duration(milliseconds: 250));
      await _addUsername(username, reportFailure: true);
    }
  }

  /// Whether the server answered at all, which decides a retry. A username it
  /// turned down is an answer.
  Future<bool> _addUsername(
    UiUsername username, {
    required bool reportFailure,
  }) async {
    final userCubit = context.read<UserCubit>();
    try {
      final added = await userCubit.addUsername(username);
      if (!mounted) return true;
      if (added) {
        _finish();
        return true;
      }
      setState(() {
        _usernameTaken = true;
        _isAddingUsername = false;
        _usernameError = AppLocalizations.of(
          context,
        ).usernameScreen_error_alreadyExists;
      });
      return true;
    } catch (_) {
      if (!mounted || !reportFailure) return false;
      setState(() => _isAddingUsername = false);
      showSnackBarStandalone(
        (loc) => SnackBar(content: Text(loc.usernameOnboarding_error)),
        tone: SnackbarTone.danger,
      );
      return false;
    }
  }

  Future<void> _pickAvatar() async {
    final registrationCubit = context.read<RegistrationCubit>();
    // Reduce image quality to re-encode the image.
    final image = await ImagePicker().pickImage(
      source: ImageSource.gallery,
      imageQuality: 99,
    );
    final bytes = await image?.readAsBytes();
    registrationCubit.setAvatar(bytes?.toImageData());
  }
}

/// The username the field opens on, derived from the display name.
String _usernameSuggestion(String displayName) {
  String suggestion;
  try {
    suggestion = usernameFromDisplay(display: displayName);
  } catch (_) {
    suggestion = displayName.trim().toLowerCase();
  }
  if (suggestion.isEmpty) {
    suggestion = 'user';
  }
  return UsernameInputFormatter.normalize(suggestion);
}

/// A step's opening lines, which are also where the server field hides.
class _Copy extends StatelessWidget {
  const _Copy({required this.text, this.onLongPress});

  final String text;
  final VoidCallback? onLongPress;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onLongPress: onLongPress,
      child: Text(text, style: Theme.of(context).textTheme.bodyMedium),
    );
  }
}

class _AvatarPicker extends StatelessWidget {
  const _AvatarPicker();

  static const double _size = 96;

  @override
  Widget build(BuildContext context) {
    final avatar = context.select(
      (RegistrationCubit cubit) => cubit.state.avatar,
    );
    final palette = SemanticPalette.of(context);

    if (avatar == null) {
      return Container(
        width: _size,
        height: _size,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          color: palette.fill.tertiary,
        ),
        alignment: Alignment.center,
        child: const IgnorePointer(
          child: IconTheme(
            data: IconThemeData(),
            child: AppIcon.imagePlus(size: 24),
          ),
        ),
      );
    }

    return ClipOval(
      child: Image(
        width: _size,
        height: _size,
        fit: BoxFit.cover,
        image: CachedMemoryImage.fromImageData(avatar),
      ),
    );
  }
}
