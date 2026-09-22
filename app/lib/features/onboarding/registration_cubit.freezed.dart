// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'registration_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$RegistrationState {

 String get domain; ImageData? get avatar; String get displayName; bool get isSigningUp; bool get isCheckingInvitationCode; String? get invitationCode;/// What the server said about signing up with it, once it was asked.
 RegistrationInfo? get registrationInfo;/// The admission session this device holds, and when it stops being
/// spendable.
 AdmissionSession? get admissionSession; DateTime? get admissionExpiresAt;
/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RegistrationStateCopyWith<RegistrationState> get copyWith => _$RegistrationStateCopyWithImpl<RegistrationState>(this as RegistrationState, _$identity);



@override
bool operator ==(Object other) {
  final _this = this as RegistrationState;
  return identical(this, other) || (other.runtimeType == runtimeType&&other is RegistrationState&&(identical(other.domain, _this.domain) || other.domain == _this.domain)&&(identical(other.avatar, _this.avatar) || other.avatar == _this.avatar)&&(identical(other.displayName, _this.displayName) || other.displayName == _this.displayName)&&(identical(other.isSigningUp, _this.isSigningUp) || other.isSigningUp == _this.isSigningUp)&&(identical(other.isCheckingInvitationCode, _this.isCheckingInvitationCode) || other.isCheckingInvitationCode == _this.isCheckingInvitationCode)&&(identical(other.invitationCode, _this.invitationCode) || other.invitationCode == _this.invitationCode)&&(identical(other.registrationInfo, _this.registrationInfo) || other.registrationInfo == _this.registrationInfo)&&(identical(other.admissionSession, _this.admissionSession) || other.admissionSession == _this.admissionSession)&&(identical(other.admissionExpiresAt, _this.admissionExpiresAt) || other.admissionExpiresAt == _this.admissionExpiresAt));
}


@override
int get hashCode {
  final _this = this as RegistrationState;
  return Object.hash(runtimeType,_this.domain,_this.avatar,_this.displayName,_this.isSigningUp,_this.isCheckingInvitationCode,_this.invitationCode,_this.registrationInfo,_this.admissionSession,_this.admissionExpiresAt);
}

@override
String toString() {
  final _this = this as RegistrationState;
  return 'RegistrationState(domain: ${_this.domain}, avatar: ${_this.avatar}, displayName: ${_this.displayName}, isSigningUp: ${_this.isSigningUp}, isCheckingInvitationCode: ${_this.isCheckingInvitationCode}, invitationCode: ${_this.invitationCode}, registrationInfo: ${_this.registrationInfo}, admissionSession: ${_this.admissionSession}, admissionExpiresAt: ${_this.admissionExpiresAt})';
}


}

/// @nodoc
abstract mixin class $RegistrationStateCopyWith<$Res>  {
  factory $RegistrationStateCopyWith(RegistrationState value, $Res Function(RegistrationState) _then) = _$RegistrationStateCopyWithImpl;
@useResult
$Res call({
 String domain, ImageData? avatar, String displayName, bool isSigningUp, bool isCheckingInvitationCode, String? invitationCode, RegistrationInfo? registrationInfo, AdmissionSession? admissionSession, DateTime? admissionExpiresAt
});


$RegistrationInfoCopyWith<$Res>? get registrationInfo;

}
/// @nodoc
class _$RegistrationStateCopyWithImpl<$Res>
    implements $RegistrationStateCopyWith<$Res> {
  _$RegistrationStateCopyWithImpl(this._self, this._then);

  final RegistrationState _self;
  final $Res Function(RegistrationState) _then;

/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? domain = null,Object? avatar = freezed,Object? displayName = null,Object? isSigningUp = null,Object? isCheckingInvitationCode = null,Object? invitationCode = freezed,Object? registrationInfo = freezed,Object? admissionSession = freezed,Object? admissionExpiresAt = freezed,}) {
  return _then(RegistrationState(
domain: null == domain ? _self.domain : domain // ignore: cast_nullable_to_non_nullable
as String,avatar: freezed == avatar ? _self.avatar : avatar // ignore: cast_nullable_to_non_nullable
as ImageData?,displayName: null == displayName ? _self.displayName : displayName // ignore: cast_nullable_to_non_nullable
as String,isSigningUp: null == isSigningUp ? _self.isSigningUp : isSigningUp // ignore: cast_nullable_to_non_nullable
as bool,isCheckingInvitationCode: null == isCheckingInvitationCode ? _self.isCheckingInvitationCode : isCheckingInvitationCode // ignore: cast_nullable_to_non_nullable
as bool,invitationCode: freezed == invitationCode ? _self.invitationCode : invitationCode // ignore: cast_nullable_to_non_nullable
as String?,registrationInfo: freezed == registrationInfo ? _self.registrationInfo : registrationInfo // ignore: cast_nullable_to_non_nullable
as RegistrationInfo?,admissionSession: freezed == admissionSession ? _self.admissionSession : admissionSession // ignore: cast_nullable_to_non_nullable
as AdmissionSession?,admissionExpiresAt: freezed == admissionExpiresAt ? _self.admissionExpiresAt : admissionExpiresAt // ignore: cast_nullable_to_non_nullable
as DateTime?,
  ));
}
/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$RegistrationInfoCopyWith<$Res>? get registrationInfo {
    if (_self.registrationInfo == null) {
    return null;
  }

  return $RegistrationInfoCopyWith<$Res>(_self.registrationInfo!, (value) {
    return _then(_self.copyWith(registrationInfo: value));
  });
}
}



/// @nodoc


class _RegistrationState extends RegistrationState {
  const _RegistrationState({this.domain = 'air.ms', this.avatar, this.displayName = '', this.isSigningUp = false, this.isCheckingInvitationCode = false, this.invitationCode, this.registrationInfo, this.admissionSession, this.admissionExpiresAt}): super._();
  

@override@JsonKey() final  String domain;
@override final  ImageData? avatar;
@override@JsonKey() final  String displayName;
@override@JsonKey() final  bool isSigningUp;
@override@JsonKey() final  bool isCheckingInvitationCode;
@override final  String? invitationCode;
/// What the server said about signing up with it, once it was asked.
@override final  RegistrationInfo? registrationInfo;
/// The admission session this device holds, and when it stops being
/// spendable.
@override final  AdmissionSession? admissionSession;
@override final  DateTime? admissionExpiresAt;

/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$RegistrationStateCopyWith<_RegistrationState> get copyWith => __$RegistrationStateCopyWithImpl<_RegistrationState>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is _RegistrationState&&(identical(other.domain, domain) || other.domain == domain)&&(identical(other.avatar, avatar) || other.avatar == avatar)&&(identical(other.displayName, displayName) || other.displayName == displayName)&&(identical(other.isSigningUp, isSigningUp) || other.isSigningUp == isSigningUp)&&(identical(other.isCheckingInvitationCode, isCheckingInvitationCode) || other.isCheckingInvitationCode == isCheckingInvitationCode)&&(identical(other.invitationCode, invitationCode) || other.invitationCode == invitationCode)&&(identical(other.registrationInfo, registrationInfo) || other.registrationInfo == registrationInfo)&&(identical(other.admissionSession, admissionSession) || other.admissionSession == admissionSession)&&(identical(other.admissionExpiresAt, admissionExpiresAt) || other.admissionExpiresAt == admissionExpiresAt));
}


@override
int get hashCode {
    return Object.hash(runtimeType,domain,avatar,displayName,isSigningUp,isCheckingInvitationCode,invitationCode,registrationInfo,admissionSession,admissionExpiresAt);
}

@override
String toString() {
    return 'RegistrationState(domain: $domain, avatar: $avatar, displayName: $displayName, isSigningUp: $isSigningUp, isCheckingInvitationCode: $isCheckingInvitationCode, invitationCode: $invitationCode, registrationInfo: $registrationInfo, admissionSession: $admissionSession, admissionExpiresAt: $admissionExpiresAt)';
}


}

/// @nodoc
abstract mixin class _$RegistrationStateCopyWith<$Res> implements $RegistrationStateCopyWith<$Res> {
  factory _$RegistrationStateCopyWith(_RegistrationState value, $Res Function(_RegistrationState) _then) = __$RegistrationStateCopyWithImpl;
@override @useResult
$Res call({
 String domain, ImageData? avatar, String displayName, bool isSigningUp, bool isCheckingInvitationCode, String? invitationCode, RegistrationInfo? registrationInfo, AdmissionSession? admissionSession, DateTime? admissionExpiresAt
});


@override $RegistrationInfoCopyWith<$Res>? get registrationInfo;

}
/// @nodoc
class __$RegistrationStateCopyWithImpl<$Res>
    implements _$RegistrationStateCopyWith<$Res> {
  __$RegistrationStateCopyWithImpl(this._self, this._then);

  final _RegistrationState _self;
  final $Res Function(_RegistrationState) _then;

/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? domain = null,Object? avatar = freezed,Object? displayName = null,Object? isSigningUp = null,Object? isCheckingInvitationCode = null,Object? invitationCode = freezed,Object? registrationInfo = freezed,Object? admissionSession = freezed,Object? admissionExpiresAt = freezed,}) {
  return _then(_RegistrationState(
domain: null == domain ? _self.domain : domain // ignore: cast_nullable_to_non_nullable
as String,avatar: freezed == avatar ? _self.avatar : avatar // ignore: cast_nullable_to_non_nullable
as ImageData?,displayName: null == displayName ? _self.displayName : displayName // ignore: cast_nullable_to_non_nullable
as String,isSigningUp: null == isSigningUp ? _self.isSigningUp : isSigningUp // ignore: cast_nullable_to_non_nullable
as bool,isCheckingInvitationCode: null == isCheckingInvitationCode ? _self.isCheckingInvitationCode : isCheckingInvitationCode // ignore: cast_nullable_to_non_nullable
as bool,invitationCode: freezed == invitationCode ? _self.invitationCode : invitationCode // ignore: cast_nullable_to_non_nullable
as String?,registrationInfo: freezed == registrationInfo ? _self.registrationInfo : registrationInfo // ignore: cast_nullable_to_non_nullable
as RegistrationInfo?,admissionSession: freezed == admissionSession ? _self.admissionSession : admissionSession // ignore: cast_nullable_to_non_nullable
as AdmissionSession?,admissionExpiresAt: freezed == admissionExpiresAt ? _self.admissionExpiresAt : admissionExpiresAt // ignore: cast_nullable_to_non_nullable
as DateTime?,
  ));
}

/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$RegistrationInfoCopyWith<$Res>? get registrationInfo {
    if (_self.registrationInfo == null) {
    return null;
  }

  return $RegistrationInfoCopyWith<$Res>(_self.registrationInfo!, (value) {
    return _then(_self.copyWith(registrationInfo: value));
  });
}
}

// dart format on
