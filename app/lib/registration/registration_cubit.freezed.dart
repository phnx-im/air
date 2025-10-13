// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'registration_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$RegistrationState {

// Domain choice screen data
 String get domain;// Display name/avatar screen data
 ImageData? get avatar; String get displayName; bool get isSigningUp;
/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$RegistrationStateCopyWith<RegistrationState> get copyWith => _$RegistrationStateCopyWithImpl<RegistrationState>(this as RegistrationState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is RegistrationState&&(identical(other.domain, domain) || other.domain == domain)&&(identical(other.avatar, avatar) || other.avatar == avatar)&&(identical(other.displayName, displayName) || other.displayName == displayName)&&(identical(other.isSigningUp, isSigningUp) || other.isSigningUp == isSigningUp));
}


@override
int get hashCode => Object.hash(runtimeType,domain,avatar,displayName,isSigningUp);

@override
String toString() {
  return 'RegistrationState(domain: $domain, avatar: $avatar, displayName: $displayName, isSigningUp: $isSigningUp)';
}


}

/// @nodoc
abstract mixin class $RegistrationStateCopyWith<$Res>  {
  factory $RegistrationStateCopyWith(RegistrationState value, $Res Function(RegistrationState) _then) = _$RegistrationStateCopyWithImpl;
@useResult
$Res call({
 String domain, ImageData? avatar, String displayName, bool isSigningUp
});




}
/// @nodoc
class _$RegistrationStateCopyWithImpl<$Res>
    implements $RegistrationStateCopyWith<$Res> {
  _$RegistrationStateCopyWithImpl(this._self, this._then);

  final RegistrationState _self;
  final $Res Function(RegistrationState) _then;

/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? domain = null,Object? avatar = freezed,Object? displayName = null,Object? isSigningUp = null,}) {
  return _then(_self.copyWith(
domain: null == domain ? _self.domain : domain // ignore: cast_nullable_to_non_nullable
as String,avatar: freezed == avatar ? _self.avatar : avatar // ignore: cast_nullable_to_non_nullable
as ImageData?,displayName: null == displayName ? _self.displayName : displayName // ignore: cast_nullable_to_non_nullable
as String,isSigningUp: null == isSigningUp ? _self.isSigningUp : isSigningUp // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}

}



/// @nodoc


class _RegistrationState extends RegistrationState {
  const _RegistrationState({this.domain = 'prod.air.ms', this.avatar, this.displayName = '', this.isSigningUp = false}): super._();
  

// Domain choice screen data
@override@JsonKey() final  String domain;
// Display name/avatar screen data
@override final  ImageData? avatar;
@override@JsonKey() final  String displayName;
@override@JsonKey() final  bool isSigningUp;

/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$RegistrationStateCopyWith<_RegistrationState> get copyWith => __$RegistrationStateCopyWithImpl<_RegistrationState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _RegistrationState&&(identical(other.domain, domain) || other.domain == domain)&&(identical(other.avatar, avatar) || other.avatar == avatar)&&(identical(other.displayName, displayName) || other.displayName == displayName)&&(identical(other.isSigningUp, isSigningUp) || other.isSigningUp == isSigningUp));
}


@override
int get hashCode => Object.hash(runtimeType,domain,avatar,displayName,isSigningUp);

@override
String toString() {
  return 'RegistrationState(domain: $domain, avatar: $avatar, displayName: $displayName, isSigningUp: $isSigningUp)';
}


}

/// @nodoc
abstract mixin class _$RegistrationStateCopyWith<$Res> implements $RegistrationStateCopyWith<$Res> {
  factory _$RegistrationStateCopyWith(_RegistrationState value, $Res Function(_RegistrationState) _then) = __$RegistrationStateCopyWithImpl;
@override @useResult
$Res call({
 String domain, ImageData? avatar, String displayName, bool isSigningUp
});




}
/// @nodoc
class __$RegistrationStateCopyWithImpl<$Res>
    implements _$RegistrationStateCopyWith<$Res> {
  __$RegistrationStateCopyWithImpl(this._self, this._then);

  final _RegistrationState _self;
  final $Res Function(_RegistrationState) _then;

/// Create a copy of RegistrationState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? domain = null,Object? avatar = freezed,Object? displayName = null,Object? isSigningUp = null,}) {
  return _then(_RegistrationState(
domain: null == domain ? _self.domain : domain // ignore: cast_nullable_to_non_nullable
as String,avatar: freezed == avatar ? _self.avatar : avatar // ignore: cast_nullable_to_non_nullable
as ImageData?,displayName: null == displayName ? _self.displayName : displayName // ignore: cast_nullable_to_non_nullable
as String,isSigningUp: null == isSigningUp ? _self.isSigningUp : isSigningUp // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

// dart format on
