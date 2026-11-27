#!/usr/bin/perl
# usage: det-hash-swap.pl <crate-path> FILE...   (crate-path: `crate` inside the library, `roundhouse` in tests/)
# Rewrites std::collections::{HashMap,HashSet} to <crate-path>::det::{HashMap,HashSet} in code only: comments
# and string/char literals (the emitters print `std::collections::HashMap` into generated Rust) are masked
# and never touched. Rewrites:
#   1. `use std::collections::{A, HashMap, B};` -> `use std::collections::{A, B};` + `use <p>::det::HashMap;`
#   2. `use std::collections::HashMap;` and `std::collections::HashMap` paths -> `<p>::det::HashMap`
#   3. `HashMap::new` / `HashSet::new` (call or fn value) -> `::default` (`new` only exists for RandomState)
#   4. `HashMap::from([..` / `HashSet::from([..` -> `::from_iter([..` (`From<[_; N]>` only exists for RandomState)
use strict; use warnings;
my $p = shift;

sub mask {
    my ($s) = @_;
    my $m = $s;
    my $n = length $s;
    pos($s) = 0;
    my $i = 0;
    my $blank = sub { my ($from, $to) = @_; substr($m, $from, $to - $from) =~ s/[^\n]/ /g; };
    while ($i < $n) {
        my $rest = substr($s, $i, 2);
        if ($rest eq '//') {
            my $e = index($s, "\n", $i); $e = $n if $e < 0;
            $blank->($i, $e); $i = $e;
        } elsif ($rest eq '/*') {
            my $depth = 1; my $j = $i + 2;
            while ($j < $n && $depth) {
                my $two = substr($s, $j, 2);
                if ($two eq '/*') { $depth++; $j += 2 } elsif ($two eq '*/') { $depth--; $j += 2 } else { $j++ }
            }
            $blank->($i, $j); $i = $j;
        } elsif (substr($s, $i) =~ /\A(b?r(#*)")/) {
            my ($open, $h) = (length $1, $2);
            my $e = index($s, '"' . $h, $i + $open); $e = $n - length($h) - 1 if $e < 0;
            my $j = $e + 1 + length $h;
            $blank->($i, $j); $i = $j;
        } elsif (substr($s, $i, 1) eq '"') {
            my $j = $i + 1;
            while ($j < $n) {
                my $c = substr($s, $j, 1);
                if ($c eq '\\') { $j += 2 } elsif ($c eq '"') { $j++; last } else { $j++ }
            }
            $blank->($i, $j); $i = $j;
        } elsif (substr($s, $i, 1) eq "'" && substr($s, $i) =~ /\A'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^\\'])'/) {
            my $len = length $&;
            $blank->($i, $i + $len); $i += $len;
        } else {
            $i++;
        }
    }
    return $m;
}

# Apply s///-style rewrites found on the masked text to the real text, from the end.
sub rewrite {
    my ($text, $re, $cb) = @_;
    my $m = mask($text);
    my @hits;
    while ($m =~ /$re/g) { push @hits, [$-[0], $+[0], [map { defined $_ ? $_ : undef } @{^CAPTURE}]]; }
    for my $h (reverse @hits) {
        my ($from, $to, $caps) = @$h;
        substr($text, $from, $to - $from) = $cb->(substr($text, $from, $to - $from), @$caps);
    }
    return $text;
}

for my $file (@ARGV) {
    open my $fh, '<', $file or die "$file: $!"; local $/; my $text = <$fh>; close $fh;
    my $orig = $text;
    $text = rewrite($text, qr/^([ \t]*)use std::collections::\{([^{}]*)\};/m, sub {
        my ($all, $ind, $items) = @_;
        my @all = grep { length } map { s/^\s+|\s+$//gr } split /,/, $items;
        my @std = grep { !/^Hash(?:Map|Set)$/ } @all;
        my @det = grep {  /^Hash(?:Map|Set)$/ } @all;
        return $all unless @det;
        my $o = "";
        $o .= "${ind}use std::collections::" . (@std > 1 ? "{" . join(", ", @std) . "}" : $std[0]) . ";\n" if @std;
        return $o . "${ind}use $p\::det::" . (@det > 1 ? "{" . join(", ", @det) . "}" : $det[0]) . ";";
    });
    $text = rewrite($text, qr/\bstd::collections::(HashMap|HashSet)\b/, sub { "$p\::det::$_[1]" });
    $text = rewrite($text, qr/\b(HashMap|HashSet)::new\b/, sub { "$_[1]::default" });
    $text = rewrite($text, qr/\b(HashMap|HashSet)::from\(\[/, sub { "$_[1]::from_iter([" });
    if ($text ne $orig) { open my $out, '>', $file or die; print $out $text; close $out; }
}
