// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'registration.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$CreateUserError {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CreateUserError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CreateUserError()';
}


}

/// @nodoc
class $CreateUserErrorCopyWith<$Res>  {
$CreateUserErrorCopyWith(CreateUserError _, $Res Function(CreateUserError) __);
}



/// @nodoc


class CreateUserError_ChallengeRequired extends CreateUserError {
  const CreateUserError_ChallengeRequired({required final  List<ChallengeKind> accepted}): _accepted = accepted,super._();
  

 final  List<ChallengeKind> _accepted;
 List<ChallengeKind> get accepted {
  if (_accepted is EqualUnmodifiableListView) return _accepted;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_accepted);
}


/// Create a copy of CreateUserError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CreateUserError_ChallengeRequiredCopyWith<CreateUserError_ChallengeRequired> get copyWith => _$CreateUserError_ChallengeRequiredCopyWithImpl<CreateUserError_ChallengeRequired>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CreateUserError_ChallengeRequired&&const DeepCollectionEquality().equals(other._accepted, _accepted));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_accepted));

@override
String toString() {
  return 'CreateUserError.challengeRequired(accepted: $accepted)';
}


}

/// @nodoc
abstract mixin class $CreateUserError_ChallengeRequiredCopyWith<$Res> implements $CreateUserErrorCopyWith<$Res> {
  factory $CreateUserError_ChallengeRequiredCopyWith(CreateUserError_ChallengeRequired value, $Res Function(CreateUserError_ChallengeRequired) _then) = _$CreateUserError_ChallengeRequiredCopyWithImpl;
@useResult
$Res call({
 List<ChallengeKind> accepted
});




}
/// @nodoc
class _$CreateUserError_ChallengeRequiredCopyWithImpl<$Res>
    implements $CreateUserError_ChallengeRequiredCopyWith<$Res> {
  _$CreateUserError_ChallengeRequiredCopyWithImpl(this._self, this._then);

  final CreateUserError_ChallengeRequired _self;
  final $Res Function(CreateUserError_ChallengeRequired) _then;

/// Create a copy of CreateUserError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? accepted = null,}) {
  return _then(CreateUserError_ChallengeRequired(
accepted: null == accepted ? _self._accepted : accepted // ignore: cast_nullable_to_non_nullable
as List<ChallengeKind>,
  ));
}


}

/// @nodoc


class CreateUserError_ChallengeRejected extends CreateUserError {
  const CreateUserError_ChallengeRejected(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CreateUserError_ChallengeRejected);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CreateUserError.challengeRejected()';
}


}




/// @nodoc


class CreateUserError_Other extends CreateUserError {
  const CreateUserError_Other({required this.message}): super._();
  

 final  String message;

/// Create a copy of CreateUserError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CreateUserError_OtherCopyWith<CreateUserError_Other> get copyWith => _$CreateUserError_OtherCopyWithImpl<CreateUserError_Other>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CreateUserError_Other&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'CreateUserError.other(message: $message)';
}


}

/// @nodoc
abstract mixin class $CreateUserError_OtherCopyWith<$Res> implements $CreateUserErrorCopyWith<$Res> {
  factory $CreateUserError_OtherCopyWith(CreateUserError_Other value, $Res Function(CreateUserError_Other) _then) = _$CreateUserError_OtherCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$CreateUserError_OtherCopyWithImpl<$Res>
    implements $CreateUserError_OtherCopyWith<$Res> {
  _$CreateUserError_OtherCopyWithImpl(this._self, this._then);

  final CreateUserError_Other _self;
  final $Res Function(CreateUserError_Other) _then;

/// Create a copy of CreateUserError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(CreateUserError_Other(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$RegistrationChallenge {

 Object get field0;



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is RegistrationChallenge&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));

@override
String toString() {
  return 'RegistrationChallenge(field0: $field0)';
}


}

/// @nodoc
class $RegistrationChallengeCopyWith<$Res>  {
$RegistrationChallengeCopyWith(RegistrationChallenge _, $Res Function(RegistrationChallenge) __);
}



/// @nodoc


class RegistrationChallenge_InvitationCode extends RegistrationChallenge {
  const RegistrationChallenge_InvitationCode(this.field0): super._();
  

@override final  String field0;

/// Create a copy of RegistrationChallenge
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RegistrationChallenge_InvitationCodeCopyWith<RegistrationChallenge_InvitationCode> get copyWith => _$RegistrationChallenge_InvitationCodeCopyWithImpl<RegistrationChallenge_InvitationCode>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is RegistrationChallenge_InvitationCode&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'RegistrationChallenge.invitationCode(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $RegistrationChallenge_InvitationCodeCopyWith<$Res> implements $RegistrationChallengeCopyWith<$Res> {
  factory $RegistrationChallenge_InvitationCodeCopyWith(RegistrationChallenge_InvitationCode value, $Res Function(RegistrationChallenge_InvitationCode) _then) = _$RegistrationChallenge_InvitationCodeCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$RegistrationChallenge_InvitationCodeCopyWithImpl<$Res>
    implements $RegistrationChallenge_InvitationCodeCopyWith<$Res> {
  _$RegistrationChallenge_InvitationCodeCopyWithImpl(this._self, this._then);

  final RegistrationChallenge_InvitationCode _self;
  final $Res Function(RegistrationChallenge_InvitationCode) _then;

/// Create a copy of RegistrationChallenge
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(RegistrationChallenge_InvitationCode(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class RegistrationChallenge_AdmissionSession extends RegistrationChallenge {
  const RegistrationChallenge_AdmissionSession(this.field0): super._();
  

@override final  AdmissionSession field0;

/// Create a copy of RegistrationChallenge
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RegistrationChallenge_AdmissionSessionCopyWith<RegistrationChallenge_AdmissionSession> get copyWith => _$RegistrationChallenge_AdmissionSessionCopyWithImpl<RegistrationChallenge_AdmissionSession>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is RegistrationChallenge_AdmissionSession&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'RegistrationChallenge.admissionSession(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $RegistrationChallenge_AdmissionSessionCopyWith<$Res> implements $RegistrationChallengeCopyWith<$Res> {
  factory $RegistrationChallenge_AdmissionSessionCopyWith(RegistrationChallenge_AdmissionSession value, $Res Function(RegistrationChallenge_AdmissionSession) _then) = _$RegistrationChallenge_AdmissionSessionCopyWithImpl;
@useResult
$Res call({
 AdmissionSession field0
});




}
/// @nodoc
class _$RegistrationChallenge_AdmissionSessionCopyWithImpl<$Res>
    implements $RegistrationChallenge_AdmissionSessionCopyWith<$Res> {
  _$RegistrationChallenge_AdmissionSessionCopyWithImpl(this._self, this._then);

  final RegistrationChallenge_AdmissionSession _self;
  final $Res Function(RegistrationChallenge_AdmissionSession) _then;

/// Create a copy of RegistrationChallenge
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(RegistrationChallenge_AdmissionSession(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as AdmissionSession,
  ));
}


}

/// @nodoc
mixin _$RegistrationInfo {

 bool get challengeRequired; List<ChallengeKind> get acceptedChallenges;
/// Create a copy of RegistrationInfo
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RegistrationInfoCopyWith<RegistrationInfo> get copyWith => _$RegistrationInfoCopyWithImpl<RegistrationInfo>(this as RegistrationInfo, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is RegistrationInfo&&(identical(other.challengeRequired, challengeRequired) || other.challengeRequired == challengeRequired)&&const DeepCollectionEquality().equals(other.acceptedChallenges, acceptedChallenges));
}


@override
int get hashCode => Object.hash(runtimeType,challengeRequired,const DeepCollectionEquality().hash(acceptedChallenges));

@override
String toString() {
  return 'RegistrationInfo(challengeRequired: $challengeRequired, acceptedChallenges: $acceptedChallenges)';
}


}

/// @nodoc
abstract mixin class $RegistrationInfoCopyWith<$Res>  {
  factory $RegistrationInfoCopyWith(RegistrationInfo value, $Res Function(RegistrationInfo) _then) = _$RegistrationInfoCopyWithImpl;
@useResult
$Res call({
 bool challengeRequired, List<ChallengeKind> acceptedChallenges
});




}
/// @nodoc
class _$RegistrationInfoCopyWithImpl<$Res>
    implements $RegistrationInfoCopyWith<$Res> {
  _$RegistrationInfoCopyWithImpl(this._self, this._then);

  final RegistrationInfo _self;
  final $Res Function(RegistrationInfo) _then;

/// Create a copy of RegistrationInfo
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? challengeRequired = null,Object? acceptedChallenges = null,}) {
  return _then(_self.copyWith(
challengeRequired: null == challengeRequired ? _self.challengeRequired : challengeRequired // ignore: cast_nullable_to_non_nullable
as bool,acceptedChallenges: null == acceptedChallenges ? _self.acceptedChallenges : acceptedChallenges // ignore: cast_nullable_to_non_nullable
as List<ChallengeKind>,
  ));
}

}



/// @nodoc


class _RegistrationInfo implements RegistrationInfo {
  const _RegistrationInfo({required this.challengeRequired, required final  List<ChallengeKind> acceptedChallenges}): _acceptedChallenges = acceptedChallenges;
  

@override final  bool challengeRequired;
 final  List<ChallengeKind> _acceptedChallenges;
@override List<ChallengeKind> get acceptedChallenges {
  if (_acceptedChallenges is EqualUnmodifiableListView) return _acceptedChallenges;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_acceptedChallenges);
}


/// Create a copy of RegistrationInfo
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$RegistrationInfoCopyWith<_RegistrationInfo> get copyWith => __$RegistrationInfoCopyWithImpl<_RegistrationInfo>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _RegistrationInfo&&(identical(other.challengeRequired, challengeRequired) || other.challengeRequired == challengeRequired)&&const DeepCollectionEquality().equals(other._acceptedChallenges, _acceptedChallenges));
}


@override
int get hashCode => Object.hash(runtimeType,challengeRequired,const DeepCollectionEquality().hash(_acceptedChallenges));

@override
String toString() {
  return 'RegistrationInfo(challengeRequired: $challengeRequired, acceptedChallenges: $acceptedChallenges)';
}


}

/// @nodoc
abstract mixin class _$RegistrationInfoCopyWith<$Res> implements $RegistrationInfoCopyWith<$Res> {
  factory _$RegistrationInfoCopyWith(_RegistrationInfo value, $Res Function(_RegistrationInfo) _then) = __$RegistrationInfoCopyWithImpl;
@override @useResult
$Res call({
 bool challengeRequired, List<ChallengeKind> acceptedChallenges
});




}
/// @nodoc
class __$RegistrationInfoCopyWithImpl<$Res>
    implements _$RegistrationInfoCopyWith<$Res> {
  __$RegistrationInfoCopyWithImpl(this._self, this._then);

  final _RegistrationInfo _self;
  final $Res Function(_RegistrationInfo) _then;

/// Create a copy of RegistrationInfo
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? challengeRequired = null,Object? acceptedChallenges = null,}) {
  return _then(_RegistrationInfo(
challengeRequired: null == challengeRequired ? _self.challengeRequired : challengeRequired // ignore: cast_nullable_to_non_nullable
as bool,acceptedChallenges: null == acceptedChallenges ? _self._acceptedChallenges : acceptedChallenges // ignore: cast_nullable_to_non_nullable
as List<ChallengeKind>,
  ));
}


}

// dart format on
