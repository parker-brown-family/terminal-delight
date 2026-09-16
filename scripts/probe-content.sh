#!/usr/bin/env bash
# Content for the snapshot probe: the shapes the generated sweep found, printed
# through a real pseudoterminal so the encoder meets them the way it does in
# production rather than in a fixture.
printf 'plain line one\n'

# A soft wrap whose continuation row is nothing but spaces. Eighty characters
# fill the row exactly; the trailing space is the continuation.
printf '%s \n' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'

# Colour painted to the margin, so the row after it is the one a scroll tints.
printf '\033[44m%-80s\033[0m\n' 'tinted to the very margin'
printf '\033[41mred to the edge %-64s\033[0m\n' ''

# Erases that leave the current background behind.
printf '\033[42msome text\033[K\n'
printf '\033[0m'

# Wide characters at and across the margin, then erases that tear them in half.
printf '%s🙂🙂\033[J\n' 'seventy-six characters of padding before the emoji pair aaaaaaaaaaaaaaaaaaaaaaaa'
printf '%s漢\n' 'a wide character that cannot fit in the last column aaaaaaaaaaaaaaaaaaaaaaaaaaa'
printf 'erase across a wide char: 漢字漢字\033[2K\n'
printf 'combining marks: e\xcc\x81x and a zero width joiner\n'

# Enough output to push most of it into scrollback, which is what the hash
# actually walks.
for i in $(seq 1 60); do
    printf 'scroll line %02d with a trailing space \n' "$i"
done

# Attributes still active when a row ends, which is the state the row-boundary
# reset was added for.
printf '\033[1;4;33mbold underlined yellow to the end %-44s\n' ''
printf '\033[0mdone\n'
