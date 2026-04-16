// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'invitation_codes_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$InvitationCodesState {

 List<UiInvitationCode> get codes;
/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InvitationCodesStateCopyWith<InvitationCodesState> get copyWith => _$InvitationCodesStateCopyWithImpl<InvitationCodesState>(this as InvitationCodesState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InvitationCodesState&&const DeepCollectionEquality().equals(other.codes, codes));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(codes));

@override
String toString() {
  return 'InvitationCodesState(codes: $codes)';
}


}

/// @nodoc
abstract mixin class $InvitationCodesStateCopyWith<$Res>  {
  factory $InvitationCodesStateCopyWith(InvitationCodesState value, $Res Function(InvitationCodesState) _then) = _$InvitationCodesStateCopyWithImpl;
@useResult
$Res call({
 List<UiInvitationCode> codes
});




}
/// @nodoc
class _$InvitationCodesStateCopyWithImpl<$Res>
    implements $InvitationCodesStateCopyWith<$Res> {
  _$InvitationCodesStateCopyWithImpl(this._self, this._then);

  final InvitationCodesState _self;
  final $Res Function(InvitationCodesState) _then;

/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? codes = null,}) {
  return _then(_self.copyWith(
codes: null == codes ? _self.codes : codes // ignore: cast_nullable_to_non_nullable
as List<UiInvitationCode>,
  ));
}

}



/// @nodoc


class _InvitationCodesState extends InvitationCodesState {
  const _InvitationCodesState({required final  List<UiInvitationCode> codes}): _codes = codes,super._();
  

 final  List<UiInvitationCode> _codes;
@override List<UiInvitationCode> get codes {
  if (_codes is EqualUnmodifiableListView) return _codes;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_codes);
}


/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$InvitationCodesStateCopyWith<_InvitationCodesState> get copyWith => __$InvitationCodesStateCopyWithImpl<_InvitationCodesState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _InvitationCodesState&&const DeepCollectionEquality().equals(other._codes, _codes));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_codes));

@override
String toString() {
  return 'InvitationCodesState(codes: $codes)';
}


}

/// @nodoc
abstract mixin class _$InvitationCodesStateCopyWith<$Res> implements $InvitationCodesStateCopyWith<$Res> {
  factory _$InvitationCodesStateCopyWith(_InvitationCodesState value, $Res Function(_InvitationCodesState) _then) = __$InvitationCodesStateCopyWithImpl;
@override @useResult
$Res call({
 List<UiInvitationCode> codes
});




}
/// @nodoc
class __$InvitationCodesStateCopyWithImpl<$Res>
    implements _$InvitationCodesStateCopyWith<$Res> {
  __$InvitationCodesStateCopyWithImpl(this._self, this._then);

  final _InvitationCodesState _self;
  final $Res Function(_InvitationCodesState) _then;

/// Create a copy of InvitationCodesState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? codes = null,}) {
  return _then(_InvitationCodesState(
codes: null == codes ? _self._codes : codes // ignore: cast_nullable_to_non_nullable
as List<UiInvitationCode>,
  ));
}


}

/// @nodoc
mixin _$UiInvitationCode {

 Object get field0;



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiInvitationCode&&const DeepCollectionEquality().equals(other.field0, field0));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(field0));

@override
String toString() {
  return 'UiInvitationCode(field0: $field0)';
}


}

/// @nodoc
class $UiInvitationCodeCopyWith<$Res>  {
$UiInvitationCodeCopyWith(UiInvitationCode _, $Res Function(UiInvitationCode) __);
}



/// @nodoc


class UiInvitationCode_Token extends UiInvitationCode {
  const UiInvitationCode_Token(this.field0): super._();
  

@override final  int field0;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiInvitationCode_TokenCopyWith<UiInvitationCode_Token> get copyWith => _$UiInvitationCode_TokenCopyWithImpl<UiInvitationCode_Token>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiInvitationCode_Token&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'UiInvitationCode.token(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $UiInvitationCode_TokenCopyWith<$Res> implements $UiInvitationCodeCopyWith<$Res> {
  factory $UiInvitationCode_TokenCopyWith(UiInvitationCode_Token value, $Res Function(UiInvitationCode_Token) _then) = _$UiInvitationCode_TokenCopyWithImpl;
@useResult
$Res call({
 int field0
});




}
/// @nodoc
class _$UiInvitationCode_TokenCopyWithImpl<$Res>
    implements $UiInvitationCode_TokenCopyWith<$Res> {
  _$UiInvitationCode_TokenCopyWithImpl(this._self, this._then);

  final UiInvitationCode_Token _self;
  final $Res Function(UiInvitationCode_Token) _then;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(UiInvitationCode_Token(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class UiInvitationCode_InvitationCode extends UiInvitationCode {
  const UiInvitationCode_InvitationCode(this.field0): super._();
  

@override final  String field0;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiInvitationCode_InvitationCodeCopyWith<UiInvitationCode_InvitationCode> get copyWith => _$UiInvitationCode_InvitationCodeCopyWithImpl<UiInvitationCode_InvitationCode>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiInvitationCode_InvitationCode&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'UiInvitationCode.invitationCode(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $UiInvitationCode_InvitationCodeCopyWith<$Res> implements $UiInvitationCodeCopyWith<$Res> {
  factory $UiInvitationCode_InvitationCodeCopyWith(UiInvitationCode_InvitationCode value, $Res Function(UiInvitationCode_InvitationCode) _then) = _$UiInvitationCode_InvitationCodeCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$UiInvitationCode_InvitationCodeCopyWithImpl<$Res>
    implements $UiInvitationCode_InvitationCodeCopyWith<$Res> {
  _$UiInvitationCode_InvitationCodeCopyWithImpl(this._self, this._then);

  final UiInvitationCode_InvitationCode _self;
  final $Res Function(UiInvitationCode_InvitationCode) _then;

/// Create a copy of UiInvitationCode
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(UiInvitationCode_InvitationCode(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
