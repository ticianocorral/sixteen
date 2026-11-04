; Cheat Genius — ROM original (homebrew) que roda no core antes do jogo.
;
; Imita o ritual do acessório de trapaça dos anos 90: o console liga com o
; "adaptador", mostra a tela de carregamento, lista os códigos e, ao confirmar,
; o jogo de verdade entra. Não há nada de Nintendo, Codemasters ou Galoob aqui:
; o código, a tela e a fonte (font8x8, domínio público) são nossos.
;
; Protocolo com o app (caixa de correio na WRAM; o app escreve depois do
; load_game e antes do primeiro frame, e lê quando MB_STATUS deixa de ser 0):
;
;   $7E:1000  MB_READY   app -> ROM   $A5 quando a lista está pronta
;   $7E:1001  MB_STATUS  ROM -> app   0 rodando, 1 jogar com cheats, 2 cancelar
;   $7E:1002  MB_COUNT   app<->ROM    nº de entradas (a ROM atualiza ao sair)
;   $7E:1004  MB_ORIG    app -> ROM   nº de entradas que o app mandou
;   $7E:1006  MB_NCUST   ROM -> app   nº de códigos novos digitados (máx. 8)
;   $7E:1010  MB_TITLE   app -> ROM   título do jogo, ASCII, zero no fim
;   $7E:1100  MB_CUST    ROM -> app   8 códigos novos, 128 bytes cada, ASCII+0
;   $7E:2000  entradas   32 bytes cada (até 3840, cruza para o banco $7F):
;                          +0 flags: bit0 ligado, bit1 personalizado,
;                                    bit2 marcado para apagar
;                          +1 descrição ASCII, zero no fim (até 30 caracteres)
;
; Montar com scripts/build_cheat_genius.sh (ca65 + ld65).

.p816
.smart

; --- constantes -------------------------------------------------------------

MB_READY  = $1000
MB_STATUS = $1001
MB_COUNT  = $1002
MB_ORIG   = $1004
MB_NCUST  = $1006
MB_TITLE  = $1010
MB_CUST   = $1100

SHADOW    = $0200          ; tilemap sombra, 2 KB, copiado por DMA no NMI
KBUF      = $0100          ; texto sendo digitado

MAXENT    = 3840
MAXCUST   = 8
MAXKEY    = 95
VISIBLE   = 20             ; linhas da lista (linhas 4..23 da tela)
LOAD_FRAMES = 160

BTN_B     = $8000
BTN_Y     = $4000
BTN_SEL   = $2000
BTN_START = $1000
BTN_UP    = $0800
BTN_DOWN  = $0400
BTN_LEFT  = $0200
BTN_RIGHT = $0100
BTN_A     = $0080
BTN_X     = $0040
BTN_L     = $0020
BTN_R     = $0010
DIRS      = BTN_UP|BTN_DOWN|BTN_LEFT|BTN_RIGHT

ST_LOAD   = 0
ST_LIST   = 1
ST_KEY    = 2
ST_DONE   = 3

; paletas (bits 10..12 do tilemap)
PAL0      = 0 << 10        ; texto
PAL1      = 1 << 10        ; barra de seleção
PAL2      = 2 << 10        ; título
PAL3      = 3 << 10        ; ligado (verde)
PAL4      = 4 << 10        ; barra de carga cheia
PAL5      = 5 << 10        ; barra de carga vazia
PAL6      = 6 << 10        ; marcado para apagar (vermelho)
PAL7      = 7 << 10        ; dicas (cinza)

; BG2: a logo (gen_logo.py). Dois tilemaps na VRAM, escolhidos por $2108.
BG2_BIG   = $04            ; tilemap em $0400: lâmpada + nome grande
BG2_SMALL = $08            ; tilemap em $0800: nome pequeno no topo

TILE_SOLID = 96
TILE_SHELL = 97

.macro POSXY col, row
    ldx #((row * 32 + col) * 2)
    stx pos
.endmacro

.macro ATTR value
    lda #value
    sta attr
.endmacro

; qual tilemap do BG2 (a logo) aparece: 0 nenhum, BG2_BIG ou BG2_SMALL
.macro LOGO value
    sep #$20
.a8
    lda #value
    sta bg2
    rep #$20
.a16
.endmacro

.macro STRP label
    lda #.loword(label)
    sta sp
.endmacro

; --- variáveis ----------------------------------------------------------------

.segment "ZEROPAGE"
frame:     .res 2
nmi_flag:  .res 1
dirty:     .res 1
state:     .res 1
           .res 1
joy:       .res 2
joy_old:   .res 2
joy_new:   .res 2
joy_rep:   .res 2
hold:      .res 2
cursor:    .res 2
top:       .res 2
count:     .res 2
on_cnt:    .res 2
ptr:       .res 4
sp:        .res 2
tmp:       .res 2
tmp2:      .res 2
attr:      .res 2
pos:       .res 2
kcur:      .res 2
klen:      .res 2
ncust:     .res 2
msg_t:     .res 2
msg_id:    .res 2
row:       .res 2
idx:       .res 2
fl:        .res 2
bg2:       .res 1
glow:      .res 2              ; só o NMI usa: gomo aceso do disco (4 = nenhum)

.segment "BSS"
; (KBUF = $0100 é usado direto; o segmento só reserva o espaço)
kbuf_res:  .res 128

; --- código ---------------------------------------------------------------------

.segment "CODE"

reset:
    sei
    clc
    xce
    rep #$38
    ldx #$0BFF
    txs
    lda #0
    tcd
    sep #$20
    lda #0
    pha
    plb
    rep #$30
.a16
.i16
    ; zera $0000..$09FF (variáveis, KBUF, tilemap sombra); a caixa de correio
    ; em $1000+ NÃO é tocada: o app a preenche antes do primeiro frame.
    ldx #0
    lda #0
@clr:
    sta $0000,x
    inx
    inx
    cpx #$0A00
    bne @clr

    sep #$20
.a8
    lda #$8F
    sta $2100              ; tela apagada
    stz $4200
    stz $420B
    stz $420C
    stz $2101
    stz $2102
    stz $2103
    stz $2105              ; modo 0
    stz $2106
    stz $2107              ; BG1: tilemap em $0000, 32x32
    lda #$11
    sta $210B              ; BG1 e BG2: tiles em $1000 (palavras)
    stz $210D
    stz $210D
    stz $210E
    stz $210E
    lda #$01
    sta $212C              ; só o BG1 (o NMI liga o BG2 quando há logo)
    stz $212D
    stz $2130
    stz $2131
    stz $2133

    ; fonte e, logo depois, os tiles da logo -> VRAM $1000
    lda #$80
    sta $2115
    rep #$20
.a16
    lda #$1000
    sta $2116
    lda #$1801             ; modo 1 (2 registradores), destino $2118
    sta $4300
    lda #.loword(font_tiles)
    sta $4302
    lda #(logo_tiles_end - font_tiles)
    sta $4305
    sep #$20
.a8
    lda #$00
    sta $4304
    lda #$01
    sta $420B

    ; paletas -> CGRAM
    stz $2121
    rep #$20
.a16
    lda #$2200             ; modo 0, destino $2122
    sta $4300
    lda #.loword(palettes)
    sta $4302
    lda #(palettes_end - palettes)
    sta $4305
    sep #$20
.a8
    lda #$00
    sta $4304
    lda #$01
    sta $420B

    ; os dois tilemaps da logo -> VRAM $0400 (BG2_BIG) e $0800 (BG2_SMALL)
    rep #$20
.a16
    lda #$0400
    sta $2116
    lda #$1801
    sta $4300
    lda #.loword(logo_maps)
    sta $4302
    lda #(logo_maps_end - logo_maps)
    sta $4305
    sep #$20
.a8
    lda #$00
    sta $4304
    lda #$01
    sta $420B

    ; paletas da logo -> CGRAM 32 (as 8 paletas do BG2 no modo 0)
    lda #32
    sta $2121
    rep #$20
.a16
    lda #$2200
    sta $4300
    lda #.loword(logo_pal)
    sta $4302
    lda #(logo_pal_end - logo_pal)
    sta $4305
    sep #$20
.a8
    lda #$00
    sta $4304
    lda #$01
    sta $420B

    rep #$30
.a16
.i16
    stz state
    jsr render_loading
    sep #$20
.a8
    jsr apply_bg2
    rep #$30
.a16
.i16
    sep #$20
.a8
    lda #$0F
    sta $2100              ; tela acesa
    lda #$81
    sta $4200              ; NMI + leitura automática do controle
    rep #$30
.a16
.i16

main_loop:
    sep #$20
.a8
@w: lda nmi_flag
    beq @w
    stz nmi_flag
    rep #$30
.a16
.i16
    jsr read_input
    sep #$20
.a8
    lda state
    rep #$30
.a16
    and #$00FF
    cmp #ST_LOAD
    bne @n1
    jsr st_load
    bra main_loop
@n1:
    cmp #ST_LIST
    bne @n2
    jsr st_list
    bra main_loop
@n2:
    cmp #ST_KEY
    bne main_loop          ; ST_DONE: nada a fazer, o app descarrega a ROM
    jsr st_key
    bra main_loop

; --- interrupções -----------------------------------------------------------------

nmi:
    rep #$30
.a16
.i16
    pha
    phx
    phy
    sep #$20
.a8
    lda $4210              ; reconhece o NMI
    lda dirty
    beq @nodma
    lda #$80
    sta $2115
    rep #$20
.a16
    stz $2116
    lda #$1801
    sta $4300
    lda #SHADOW
    sta $4302
    lda #$0800
    sta $4305
    sep #$20
.a8
    lda #$7E
    sta $4304
    lda #$01
    sta $420B
    stz dirty
@nodma:
    jsr apply_bg2
    jsr disc_glow
    ; cor do título (paleta 2, cor 1 = entrada 9) gira numa tabela
    lda #9
    sta $2121
    lda frame
    lsr a
    lsr a
    and #$0F
    asl a
    rep #$20
.a16
    and #$00FF
    tax
    sep #$20
.a8
    lda title_cols,x
    sta $2122
    lda title_cols+1,x
    sta $2122
    rep #$20
.a16
    inc frame
    sep #$20
.a8
    lda #1
    sta nmi_flag
    rep #$30
.a16
.i16
    ply
    plx
    pla
    rti

dummy_int:
    rti

; O disco do Genius: na tela grande os gomos acendem um de cada vez, em
; sentido horário, como o brinquedo mostrando a sequência; na lista ficam
; todos apagados. Reescreve as cores 2 e 3 das quatro paletas dos gomos.
disc_glow:
.a8
.i16
    rep #$20
.a16
    lda #4
    sta glow
    sep #$20
.a8
    lda bg2
    cmp #BG2_BIG
    bne @go
    rep #$20
.a16
    lda frame
    lsr a
    lsr a
    lsr a
    lsr a
    and #3
    sta glow
@go:
    rep #$20
.a16
    ldy #0
@q: tya
    asl a
    asl a
    asl a
    tax                    ; X = gomo * 8 (4 palavras por gomo)
    cpy glow
    bne @dim
    inx                    ; aceso: as duas palavras de depois
    inx
    inx
    inx
@dim:
    sep #$20
.a8
    lda disc_cg,y
    sta $2121
    lda disc_cols,x
    sta $2122
    lda disc_cols+1,x
    sta $2122
    lda disc_cols+2,x
    sta $2122
    lda disc_cols+3,x
    sta $2122
    rep #$20
.a16
    iny
    cpy #4
    bne @q
    sep #$20
.a8
    rts

; liga o BG2 no tilemap de `bg2`, ou só o BG1 se for 0 (A em 8 bits)
apply_bg2:
.a8
    lda bg2
    beq @off
    sta $2108
    lda #$03
    sta $212C
    rts
@off:
    lda #$01
    sta $212C
    rts

; --- entrada -------------------------------------------------------------------------

read_input:
.a16
.i16
    sep #$20
.a8
@w: lda $4212
    and #$01
    bne @w
    rep #$20
.a16
    lda joy
    sta joy_old
    lda $4218
    sta joy
    eor joy_old
    and joy
    sta joy_new
    sta joy_rep
    lda joy
    and #DIRS
    sta tmp
    beq @zero
    lda joy_old
    and #DIRS
    cmp tmp
    bne @zero
    inc hold
    lda hold
    cmp #16
    bcc @done
    and #3
    bne @done
    lda joy_rep
    ora tmp
    sta joy_rep
    bra @done
@zero:
    stz hold
@done:
    rts

; --- desenho -----------------------------------------------------------------------

; A = índice do caractere ASCII -> A = palavra do tilemap (com `attr`)
char_tile:
.a16
.i16
    sec
    sbc #$20
    cmp #$60
    bcc @ok
    lda #$1F               ; '?'
@ok:
    ora attr
    rts

; escreve a palavra A em SHADOW+pos e avança
put_ch:
.a16
.i16
    ldx pos
    sta SHADOW,x
    inx
    inx
    stx pos
    rts

clear_map:
.a16
.i16
    ldx #0
    lda #0
@l: sta SHADOW,x
    inx
    inx
    cpx #$0800
    bne @l
    rts

; desenha a string da ROM em `sp` na posição `pos`, com `attr`
draw_rom:
.a16
.i16
    ldy #0
@l: lda (sp),y
    and #$00FF
    beq @d
    jsr char_tile
    jsr put_ch
    iny
    bra @l
@d: rts

; desenha a string em [ptr] a partir de Y, no máximo `tmp2` caracteres
draw_long:
.a16
.i16
@l: lda tmp2
    beq @d
    lda [ptr],y
    and #$00FF
    beq @d
    jsr char_tile
    jsr put_ch
    iny
    dec tmp2
    bra @l
@d: rts

; desenha a string em KBUF (até o zero), no máximo `tmp2` caracteres
draw_kbuf_line:
.a16
.i16
@l: lda tmp2
    beq @d
    lda KBUF,y
    and #$00FF
    beq @d
    jsr char_tile
    jsr put_ch
    iny
    dec tmp2
    bra @l
@d: rts

; preenche a linha inteira cuja posição já está em `pos` (múltiplo de 64)
; com espaços do atributo atual — a barra de seleção
fill_row:
.a16
.i16
    ldy #32
@l: lda attr
    jsr put_ch
    dey
    bne @l
    rts

; A = valor (0..9999) -> 4 dígitos alinhados à direita
draw_num4:
.a16
.i16
    sta tmp
    stz tmp2               ; já saiu do zero à esquerda?
    ldy #0
@dig:
    ldx #0
@sub:
    lda tmp
    cmp divs,y
    bcc @emit
    sbc divs,y
    sta tmp
    inx
    bra @sub
@emit:
    cpx #0
    bne @show
    lda tmp2
    bne @show
    cpy #6
    beq @show
    lda #0                 ; espaço
    ora attr
    bra @put
@show:
    lda #1
    sta tmp2
    txa
    clc
    adc #('0' - $20)
    ora attr
@put:
    phy
    jsr put_ch
    ply
    iny
    iny
    cpy #8
    bne @dig
    rts

; A = índice -> `ptr` = $7E:2000 + índice*32
entry_ptr:
.a16
.i16
    sta tmp
    asl a
    asl a
    asl a
    asl a
    asl a
    clc
    adc #$2000
    sta ptr
    lda #0
    rol a
    sta tmp2
    lda tmp
    xba
    and #$00FF
    lsr a
    lsr a
    lsr a
    clc
    adc tmp2
    clc
    adc #$7E
    sta ptr+2
    rts

mark_dirty:
.a16
    sep #$20
.a8
    lda #1
    sta dirty
    rep #$20
.a16
    rts

; --- tela de carga -------------------------------------------------------------------

render_loading:
.a16
.i16
    jsr clear_map
    LOGO BG2_BIG
    ATTR PAL7
    POSXY 12, 13
    STRP s_loading
    jsr draw_rom
    ; barra de progresso: 20 células
    lda frame
    lsr a
    lsr a
    lsr a
    cmp #21
    bcc @c
    lda #20
@c: sta tmp
    POSXY 6, 16
    ldy #0
@bar:
    cpy tmp
    bcs @empty
    lda #(TILE_SOLID | PAL4)
    bra @p
@empty:
    lda #(TILE_SHELL | PAL5)
@p: phy
    jsr put_ch
    ply
    iny
    cpy #20
    bne @bar
    ATTR PAL7
    POSXY 9, 19
    STRP s_engine
    jsr draw_rom
    jsr mark_dirty
    rts

st_load:
.a16
.i16
    lda frame
    and #3
    bne @skip
    jsr render_loading
@skip:
    lda frame
    cmp #LOAD_FRAMES
    bcc @r
    sep #$20
.a8
    lda MB_READY
    cmp #$A5
    rep #$20
.a16
    bne @r
    jsr init_list
    sep #$20
.a8
    lda #ST_LIST
    sta state
    rep #$20
.a16
    jsr render_list
@r: rts

init_list:
.a16
.i16
    lda MB_COUNT
    cmp #(MAXENT + 1)
    bcc @ok
    lda #MAXENT
@ok:
    sta count
    stz cursor
    stz top
    stz on_cnt
    stz ncust
    stz msg_t
    lda count
    beq @e
    ldx #0
@l: txa
    phx
    jsr entry_ptr
    sep #$20
.a8
    lda [ptr]
    and #$01
    rep #$20
.a16
    plx
    and #$00FF
    beq @n
    inc on_cnt
@n: inx
    cpx count
    bne @l
@e: rts

; --- lista -----------------------------------------------------------------------------

render_list:
.a16
.i16
    jsr clear_map
    ; título: a logo pequena (BG2) nas linhas 0-1
    LOGO BG2_SMALL
    ; jogo
    ATTR PAL7
    POSXY 1, 2
    ldy #0
    lda #30
    sta tmp2
@gt:
    lda tmp2
    beq @gd
    lda MB_TITLE,y
    and #$00FF
    beq @gd
    jsr char_tile
    jsr put_ch
    iny
    dec tmp2
    bra @gt
@gd:
    ; contagem
    ATTR PAL0
    POSXY 1, 3
    STRP s_on
    jsr draw_rom
    lda on_cnt
    jsr draw_num4
    STRP s_of
    jsr draw_rom
    lda count
    jsr draw_num4
    ; linhas
    stz row
@row:
    lda row
    clc
    adc top
    cmp count
    bcc :+
    jmp @rows_done
:
    sta idx
    ; posição da linha: (4 + r) * 64
    lda row
    clc
    adc #4
    asl a
    asl a
    asl a
    asl a
    asl a
    asl a
    sta pos
    lda idx
    jsr entry_ptr
    sep #$20
.a8
    lda [ptr]
    sta fl
    stz fl+1
    rep #$20
.a16
    lda idx
    cmp cursor
    bne @notsel
    lda #PAL1
    sta attr
    jsr fill_row
    bra @marker
@notsel:
    lda fl
    and #$0004
    beq @chk_on
    lda #PAL6
    sta attr
    bra @marker
@chk_on:
    lda fl
    and #$0001
    beq @plain
    lda #PAL3
    sta attr
    bra @marker
@plain:
    lda #PAL0
    sta attr
@marker:
    ; "[x]" na coluna 1
    lda row
    clc
    adc #4
    asl a
    asl a
    asl a
    asl a
    asl a
    asl a
    clc
    adc #2
    sta pos
    lda #('[' - $20)
    ora attr
    jsr put_ch
    lda fl
    and #$0004
    beq @m1
    lda #('-' - $20)
    bra @mc
@m1:
    lda fl
    and #$0001
    beq @m0
    lda #('x' - $20)
    bra @mc
@m0:
    lda #0
@mc:
    ora attr
    jsr put_ch
    lda #(']' - $20)
    ora attr
    jsr put_ch
    lda #0
    ora attr
    jsr put_ch
    ; descrição, no máximo 26 caracteres
    lda #26
    sta tmp2
    ldy #1
    jsr draw_long
    inc row
    lda row
    cmp #VISIBLE
    bcs @rows_done
    jmp @row
@rows_done:
    ; indicadores de rolagem
    ATTR PAL7
    lda top
    beq @noup
    POSXY 0, 4
    lda #('^' - $20)
    ora attr
    jsr put_ch
@noup:
    lda top
    clc
    adc #VISIBLE
    cmp count
    bcs @nodown
    POSXY 0, 23
    lda #('v' - $20)
    ora attr
    jsr put_ch
@nodown:
    lda count
    bne @hints
    ATTR PAL7
    POSXY 3, 8
    STRP s_empty
    jsr draw_rom
@hints:
    ATTR PAL7
    POSXY 1, 25
    STRP s_hint1
    jsr draw_rom
    POSXY 1, 26
    STRP s_hint2
    jsr draw_rom
    jsr mark_dirty
    rts

; mantém o cursor dentro da janela visível
fix_scroll:
.a16
.i16
    lda cursor
    cmp top
    bcs @a
    sta top
@a: lda top
    clc
    adc #VISIBLE
    cmp cursor
    beq @b
    bcs @c
@b: lda cursor
    sec
    sbc #(VISIBLE - 1)
    sta top
@c: rts

st_list:
.a16
.i16
    lda joy_rep
    and #BTN_UP
    beq @n_up
    lda cursor
    beq @n_up
    dec cursor
    brl @moved
@n_up:
    lda joy_rep
    and #BTN_DOWN
    beq @n_down
    lda cursor
    inc a
    cmp count
    bcs @n_down
    sta cursor
    brl @moved
@n_down:
    lda joy_rep
    and #BTN_L
    beq @n_l
    lda cursor
    sec
    sbc #(VISIBLE - 1)
    bcs @lset
    lda #0
@lset:
    sta cursor
    brl @moved
@n_l:
    lda joy_rep
    and #BTN_R
    beq @n_r
    lda count
    beq @n_r
    lda cursor
    clc
    adc #(VISIBLE - 1)
    cmp count
    bcc @rset
    lda count
    dec a
@rset:
    sta cursor
    brl @moved
@n_r:
    lda joy_new
    and #BTN_A
    beq @n_a
    lda count
    beq @n_a
    lda cursor
    jsr entry_ptr
    sep #$20
.a8
    lda [ptr]
    eor #$01
    sta [ptr]
    rep #$20
.a16
    and #$0001
    beq @off
    inc on_cnt
    bra @redraw
@off:
    dec on_cnt
    bra @redraw
@n_a:
    lda joy_new
    and #BTN_X
    beq @n_x
    lda count
    beq @n_x
    lda cursor
    jsr entry_ptr
    sep #$20
.a8
    lda [ptr]
    bit #$02
    beq @notcust
    eor #$04
    sta [ptr]
@notcust:
    rep #$20
.a16
    bra @redraw
@n_x:
    lda joy_new
    and #BTN_Y
    beq @n_y
    stz klen
    sep #$20
.a8
    stz KBUF
    rep #$20
.a16
    stz kcur
    stz msg_t
    sep #$20
.a8
    lda #ST_KEY
    sta state
    rep #$20
.a16
    jmp render_key
@n_y:
    lda joy_new
    and #BTN_START
    beq @n_s
    lda #1
    jmp finish
@n_s:
    lda joy_new
    and #BTN_B
    beq @ret
    lda #2
    jmp finish
@moved:
    jsr fix_scroll
@redraw:
    jmp render_list
@ret:
    rts

; A = status (1 jogar, 2 cancelar) -> publica o resultado e para
finish:
.a16
.i16
    pha
    lda count
    sta MB_COUNT
    lda ncust
    sta MB_NCUST
    jsr clear_map
    LOGO BG2_BIG
    ATTR PAL2
    POSXY 11, 14
    STRP s_start
    jsr draw_rom
    jsr mark_dirty
    pla
    sep #$20
.a8
    sta MB_STATUS
    lda #ST_DONE
    sta state
    rep #$20
.a16
    rts

; --- teclado de códigos ------------------------------------------------------------------

render_key:
.a16
.i16
    jsr clear_map
    LOGO 0
    ATTR PAL2
    POSXY 0, 1
    jsr fill_row
    POSXY 11, 1
    STRP s_newcode
    jsr draw_rom
    ; texto digitado: linhas de 28 caracteres, até 4 linhas, e o cursor '_'
    ATTR PAL3
    ldy #0
    ldx #0
@ln:
    phx
    phy
    txa
    asl a
    asl a
    asl a
    asl a
    asl a
    asl a
    clc
    adc #((4 * 32 + 2) * 2)
    sta pos
    ply
    lda #28
    sta tmp2
    jsr draw_kbuf_line
    plx
    inx
    cpx #4
    bne @ln
    ; cursor piscando logo depois do último caractere
    lda frame
    and #$0020
    beq @nocur
    lda klen
    sta tmp
    ldx #0
@cl:
    lda tmp
    cmp #28
    bcc @cdone
    sbc #28
    sta tmp
    inx
    bra @cl
@cdone:
    txa
    asl a
    asl a
    asl a
    asl a
    asl a
    asl a
    clc
    adc #((4 * 32 + 2) * 2)
    sta pos
    lda tmp
    asl a
    clc
    adc pos
    sta pos
    ATTR PAL3
    lda #('_' - $20)
    ora attr
    jsr put_ch
@nocur:
    ; grade 6x3 de caracteres
    ldx #0
@key:
    stx tmp
    ; posição: linha 10 + (k/6)*2, coluna 3 + (k%6)*4
    txa
    ldy #0
@div:
    cmp #6
    bcc @dd
    sbc #6
    iny
    bra @div
@dd:
    ; A = col índice, Y = linha índice
    asl a
    asl a
    clc
    adc #3
    sta tmp2
    tya
    asl a
    clc
    adc #10
    asl a
    asl a
    asl a
    asl a
    asl a
    asl a                  ; linha * 64 bytes
    sta pos
    lda tmp2
    asl a
    clc
    adc pos
    sta pos
    lda kcur
    cmp tmp
    bne @kn
    lda #PAL1
    bra @ka
@kn:
    lda #PAL0
@ka:
    sta attr
    lda #0
    ora attr
    jsr put_ch
    ldx tmp
    lda keychars,x
    and #$00FF
    jsr char_tile
    jsr put_ch
    lda #0
    ora attr
    jsr put_ch
    ldx tmp
    inx
    cpx #18
    bne @key
    ; botões DEL / CLEAR / OK / BACK  (kcur 18..21)
    lda kcur
    cmp #18
    bne @b1
    lda #PAL1
    bra @b1s
@b1:
    lda #PAL0
@b1s:
    sta attr
    POSXY 2, 17
    STRP s_del
    jsr draw_rom
    lda kcur
    cmp #19
    bne @b2
    lda #PAL1
    bra @b2s
@b2:
    lda #PAL0
@b2s:
    sta attr
    POSXY 9, 17
    STRP s_clear
    jsr draw_rom
    lda kcur
    cmp #20
    bne @b3
    lda #PAL1
    bra @b3s
@b3:
    lda #PAL0
@b3s:
    sta attr
    POSXY 18, 17
    STRP s_ok
    jsr draw_rom
    lda kcur
    cmp #21
    bne @b4
    lda #PAL1
    bra @b4s
@b4:
    lda #PAL0
@b4s:
    sta attr
    POSXY 24, 17
    STRP s_back
    jsr draw_rom
    ; mensagem de erro e dicas
    lda msg_t
    beq @nomsg
    ATTR PAL6
    POSXY 3, 20
    lda msg_id
    cmp #1
    bne @m2
    STRP s_invalid
    bra @mdraw
@m2:
    STRP s_limit
@mdraw:
    jsr draw_rom
@nomsg:
    ATTR PAL7
    POSXY 1, 22
    STRP s_kh1
    jsr draw_rom
    POSXY 1, 23
    STRP s_kh2
    jsr draw_rom
    POSXY 1, 25
    STRP s_kh3
    jsr draw_rom
    POSXY 1, 26
    STRP s_kh4
    jsr draw_rom
    jsr mark_dirty
    rts

st_key:
.a16
.i16
    ; o cursor piscante e a mensagem pedem redesenho periódico
    lda msg_t
    beq @nomsg
    dec msg_t
@nomsg:
    lda joy_rep
    and #BTN_LEFT
    beq @n_l
    lda kcur
    bne @l1
    lda #22
@l1:
    dec a
    sta kcur
    brl @redraw
@n_l:
    lda joy_rep
    and #BTN_RIGHT
    beq @n_r
    lda kcur
    inc a
    cmp #22
    bcc @r1
    lda #0
@r1:
    sta kcur
    brl @redraw
@n_r:
    lda joy_rep
    and #BTN_UP
    beq @n_u
    lda kcur
    cmp #6
    bcs @u1
    clc
    adc #18
    cmp #22
    bcc @u2
    lda #21
    bra @u2
@u1:
    sec
    sbc #6
@u2:
    sta kcur
    brl @redraw
@n_u:
    lda joy_rep
    and #BTN_DOWN
    beq @n_d
    lda kcur
    clc
    adc #6
    cmp #24
    bcc @d1
    sec
    sbc #24
    bra @d2
@d1:
    cmp #22
    bcc @d2
    lda #21
@d2:
    sta kcur
    bra @redraw
@n_d:
    lda joy_new
    and #BTN_A
    beq @n_a
    lda kcur
    cmp #18
    bcc @type
    beq @del
    cmp #19
    beq @clear
    cmp #20
    beq @ok
    bra @back
@type:
    lda klen
    cmp #MAXKEY
    bcs @redraw
    tax
    lda kcur
    tay
    sep #$20
.a8
    lda keychars,y
    sta KBUF,x
    inx
    lda #0
    sta KBUF,x
    rep #$20
.a16
    stx klen
    bra @redraw
@del:
    jsr key_del
    bra @redraw
@clear:
    stz klen
    sep #$20
.a8
    stz KBUF
    rep #$20
.a16
    bra @redraw
@ok:
    jmp key_ok
@back:
    jmp leave_key
@n_a:
    lda joy_new
    and #BTN_B
    beq @n_b
    jsr key_del
    bra @redraw
@n_b:
    lda joy_new
    and #BTN_START
    beq @n_s
    jmp key_ok
@n_s:
    lda joy_new
    and #BTN_SEL
    beq @blink
    jmp leave_key
@blink:
    ; redesenho do cursor piscante, a cada 16 quadros
    lda frame
    and #$000F
    bne @r
@redraw:
    jmp render_key
@r: rts

key_del:
.a16
.i16
    lda klen
    beq @r
    dec a
    sta klen
    tax
    sep #$20
.a8
    lda #0
    sta KBUF,x
    rep #$20
.a16
@r: rts

leave_key:
.a16
.i16
    sep #$20
.a8
    lda #ST_LIST
    sta state
    rep #$20
.a16
    jmp render_list

; confirma o código digitado: valida, guarda e põe na lista
key_ok:
.a16
.i16
    lda klen
    bne :+
    jmp @ret
:
    jsr validate_kbuf
    bcs @valid
    lda #60
    sta msg_t
    lda #1
    sta msg_id
    jmp render_key
@valid:
    lda ncust
    cmp #MAXCUST
    bcs @limit
    lda count
    cmp #MAXENT
    bcc @room
@limit:
    lda #60
    sta msg_t
    lda #2
    sta msg_id
    jmp render_key
@room:
    ; copia o código para o slot ncust*128 da caixa de correio
    lda ncust
    xba
    lsr a                  ; *128
    clc
    adc #MB_CUST
    sta sp
    ldy #0
    sep #$20
.a8
@cp:
    lda KBUF,y
    sta (sp),y
    beq @cpd
    iny
    bra @cp
@cpd:
    rep #$20
.a16
    ; nova entrada: ligada + personalizada, descrição = começo do código
    lda count
    jsr entry_ptr
    sep #$20
.a8
    lda #$03
    sta [ptr]
    ldx #0
    ldy #1
@dc:
    cpx #30
    beq @dt
    lda KBUF,x
    beq @dt
    sta [ptr],y
    inx
    iny
    bra @dc
@dt:
    lda #0
    sta [ptr],y
    rep #$20
.a16
    inc count
    inc ncust
    inc on_cnt
    lda count
    dec a
    sta cursor
    jsr fix_scroll
    jmp leave_key
@ret:
    rts

; C=1 se KBUF é uma lista válida de partes "XXXX-XXXX" (Game Genie) ou
; "XXXXXXXX" (RAM), separadas por '+'
validate_kbuf:
.a16
.i16
    ldx #0
    stz tmp                ; comprimento da parte
    lda #$FFFF
    sta tmp2               ; posição do '-' (ou $FFFF)
@l:
    sep #$20
.a8
    lda KBUF,x
    rep #$20
.a16
    and #$00FF
    beq @end
    cmp #'+'
    beq @plus
    cmp #'-'
    bne @ch
    lda tmp2
    cmp #$FFFF
    bne @bad               ; dois hífens
    lda tmp
    sta tmp2
@ch:
    inc tmp
    inx
    bra @l
@plus:
    jsr part_ok
    bcc @bad
    stz tmp
    lda #$FFFF
    sta tmp2
    inx
    bra @l
@end:
    jsr part_ok
    rts
@bad:
    clc
    rts

part_ok:
.a16
.i16
    lda tmp
    cmp #8
    bne @nine
    lda tmp2
    cmp #$FFFF
    bne @no
    sec
    rts
@nine:
    cmp #9
    bne @no
    lda tmp2
    cmp #4
    bne @no
    sec
    rts
@no:
    clc
    rts

; --- dados -----------------------------------------------------------------------------

.segment "RODATA"

divs:      .word 1000, 100, 10, 1

keychars:  .byte "0123456789ABCDEF-+"

s_loading: .byte "LOADING...", 0
s_engine:  .byte "CODE ENGINE  V1", 0
s_on:      .byte "ON", 0
s_of:      .byte "  OF ", 0
s_empty:   .byte "NO CODES FOR THIS GAME", 0
s_hint1:   .byte "A ON/OFF  X DEL  Y NEW CODE", 0
s_hint2:   .byte "START PLAY  B SKIP  L/R PAGE", 0
s_start:   .byte "STARTING...", 0
s_newcode: .byte "NEW CODE", 0
s_del:     .byte " DEL ", 0
s_clear:   .byte " CLEAR ", 0
s_ok:      .byte " OK ", 0
s_back:    .byte " BACK ", 0
s_invalid: .byte "INVALID CODE", 0
s_limit:   .byte "NO MORE ROOM", 0
s_kh1:     .byte "GAME GENIE  ABCD-1234", 0
s_kh2:     .byte "RAM         7E001234", 0
s_kh3:     .byte "JOIN SEVERAL WITH +", 0
s_kh4:     .byte "A PICK B DEL START OK SEL BACK", 0

; cores do título girando (BGR555)
title_cols:
    .word $7FFF, $7FF0, $7FE0, $4FE0, $2FE0, $0FE8, $03FF, $03BF
    .word $037F, $2B5F, $4F1F, $6C1F, $7C1F, $7C5F, $7EBF, $7FDF

palettes:
    ; 0 texto
    .word $1400, $7FFF, $1400, $1400
    ; 1 barra de seleção
    .word $1400, $0000, $039F, $039F
    ; 2 título
    .word $1400, $7FFF, $3000, $3000
    ; 3 ligado
    .word $1400, $03E0, $1400, $1400
    ; 4 barra de carga cheia
    .word $1400, $7FE0, $1400, $1400
    ; 5 barra de carga vazia
    .word $1400, $7FFF, $2108, $2108
    ; 6 apagar
    .word $1400, $001F, $1400, $1400
    ; 7 dicas
    .word $1400, $5294, $1400, $1400
palettes_end:

font_inc_start:
.include "font.inc"
.include "logo.inc"

; --- cabeçalho e vetores -----------------------------------------------------------

.segment "HEADER"
    .byte "CHEAT GENIUS         "   ; 21 bytes
    .byte $20                       ; LoROM
    .byte $00                       ; sem coprocessador
    .byte $05                       ; 32 KB
    .byte $00                       ; sem SRAM
    .byte $01                       ; região: EUA/NTSC
    .byte $33
    .byte $00
    .word $FFFF                     ; complemento do checksum (build o corrige)
    .word $0000                     ; checksum

.segment "VECTORS"
    .word 0, 0
    .word dummy_int                 ; COP  (nativo)
    .word dummy_int                 ; BRK
    .word dummy_int                 ; ABORT
    .word nmi                       ; NMI
    .word 0
    .word dummy_int                 ; IRQ
    .word 0, 0
    .word dummy_int                 ; COP  (emulação)
    .word 0
    .word dummy_int                 ; ABORT
    .word nmi                       ; NMI
    .word reset                     ; RESET
    .word dummy_int                 ; IRQ/BRK
