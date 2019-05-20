// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

package ir

import (
	"testing"

	"github.com/google/go-cmp/cmp"

	"fidl/compiler/backend/types"
	"fidl/compiler/backend/typestest"
)

func TestCompileStruct(t *testing.T) {
	var c compiler
	cases := []struct {
		input    types.Struct
		expected result
	}{
		{
			input: types.Struct{},
			expected: result{
				OutOfLine: []StructMember{
					{
						Name: "void",
						Type: Type("void"),
					},
				},
				Handles: []StructMember{
					{
						Name: "void",
						Type: Type("void"),
					},
				},
			},
		},
		{
			input: types.Struct{
				Members: []types.StructMember{
					{
						Type: typestest.PrimitiveType(types.Bool),
						Name: "TheFieldName",
					},
				},
			},
			expected: result{
				Inline: []StructMember{
					{
						Name: "the_field_name",
						Type: Type("int8"),
					},
				},
				OutOfLine: []StructMember{
					{
						Name: "void",
						Type: Type("void"),
					},
				},
				Handles: []StructMember{
					{
						Name: "void",
						Type: Type("void"),
					},
				},
			},
		},
	}
	for _, ex := range cases {
		i, o, h := c.compileStruct(ex.input)
		actual := result{i, o, h}
		if diff := cmp.Diff(ex.expected, actual); diff != "" {
			t.Errorf("expected != actual (-want +got)\n%s", diff)
		}
	}
}
