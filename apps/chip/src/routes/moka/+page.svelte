<script lang="ts">
  import { writable } from 'svelte/store';
  import { browser } from '$app/environment';
  import Editor from '$lib/components/Editor.svelte';
  import type { LtLResult, MarkerData } from 'chip-wasm';
  import Nav from '$lib/components/Nav.svelte';
  import Network from '$lib/components/Network.svelte';
  import Icon from '~icons/heroicons/globe-europe-africa';
  import { mirage } from 'ayu';

  let program = `> x = 30
do
x >= 0 -> x := x-1
od
`;

  let checks = `check F x = -1              // should hold
check F x = -2              // should not hold
check G x = -1              // should not hold
check ! F ! (x = -1)        // should not hold
check ! (true U ! (x = -1)) // should not hold
check G x >= -1             // should hold
check ! F ! (x >= -1)       // should hold
`;

  // moka still wants one file, so glue the boxes back together before parsing.
  // checkOffset = how many lines the program eats, anything below that came from the check box
  $: source = `${program}\n${checks}`;
  $: checkOffset = program.split('\n').length;

  let result = writable<LtLResult>({
    parse_error: false,
    markers: [],
    ts_dot: '',
    ts_map: new Map(),
    buchi_dot: '',
    negated_nnf_ltl_property_str: '',
    buchi_property_dot: '',
    gbuchi_property_dot: '',
    kripke_str: '',
    product_ba_dot: '',
  });
  let verifications = writable<MarkerData[]>([]);

  let parseError = writable(false);

  const STATUS = ['idle', 'checking', 'checked', 'error'];
  type Status = (typeof STATUS)[number];
  let status = writable<Status>('idle');

  $: graphs = [
    { title: 'Kripke structure', dot: $result.kripke_str },
    { title: 'Buchi automaton', dot: $result.buchi_dot },
    {
      title: `Generalized Buchi property: ${$result.negated_nnf_ltl_property_str}`,
      dot: $result.gbuchi_property_dot,
    },
    {
      title: `Buchi property: ${$result.negated_nnf_ltl_property_str}`,
      dot: $result.buchi_property_dot,
    },
    { title: 'Product automaton', dot: $result.product_ba_dot },
  ];

  let hoveredNode: string | null = null;
  let hoveredMarker: number | null = null;

  let hoverMakers: MarkerData[] = [];

  $: if (typeof hoveredNode == 'number' || typeof hoveredNode == 'string') {
    const spans = $result.ts_map.get(hoveredNode.toString());
    if (spans) {
      hoverMakers = spans.map(
        (span): MarkerData => ({
          relatedInformation: [],
          tags: [],
          severity: 'Info',
          message: 'here',
          span,
        }),
      );
    } else {
      hoverMakers = [];
    }
  } else {
    hoverMakers = [];
  }
  // markers are numbered against the glued source, so send each one back to its own box.
  // the check ones keep their index so hovering a failed check still lights up the states
  $: checkMarkers = $result.markers
    .map((m, i) => ({ i, m: m[0] }))
    .filter(({ m }) => m.span.startLineNumber > checkOffset)
    .map(({ i, m }) => ({
      i,
      m: {
        ...m,
        span: {
          ...m.span,
          startLineNumber: m.span.startLineNumber - checkOffset,
          endLineNumber: m.span.endLineNumber - checkOffset,
        },
      },
    }));

  $: programMarkers = [
    ...$result.markers.map((m) => m[0]).filter((m) => m.span.startLineNumber <= checkOffset),
    ...$verifications,
    ...hoverMakers,
  ];

  $: hoveredCheck = typeof hoveredMarker == 'number' ? checkMarkers[hoveredMarker]?.i : undefined;
  $: highlightedNodes =
    (typeof hoveredCheck == 'number' && $result.markers[hoveredCheck]?.[1]) || [];

  $: if (browser) {
    const run = async () => {
      status.set('checked');
      parseError.set(false);
      const { default: init, parse_ltl } = await import('chip-wasm');
      await init();
      console.time('run wasm');
      const res = parse_ltl(source);
      console.timeEnd('run wasm');
      if (res.parse_error) parseError.set(true);
      result.set(res);
      if (res.markers.length > 0) {
        status.set('error');
      } else {
        status.set('checked');
      }
    };
    run().catch(console.error);
  }

  // the wasm side does the work. assigning to program re-triggers the check above by itself
  let preset = 'default';
  let generating = false;
  let generated: { seed: number; attempts: number } | null = null;
  let generateError: string | null = null;

  const generateProgram = async () => {
    generating = true;
    generateError = null;
    generated = null;
    try {
      const { default: init, generate_program } = await import('chip-wasm');
      await init();
      // seed from js so it stays a plain number, and so chip generate can reuse it
      const seed = Math.floor(Math.random() * 2 ** 32);
      // sampling blocks the page, let the button repaint as disabled first
      await new Promise((resolve) => setTimeout(resolve, 0));
      console.time('generate wasm');
      const res = generate_program(seed, preset);
      console.timeEnd('generate wasm');
      if (res.accepted) {
        program = res.program;
        generated = { seed: res.seed, attempts: res.attempts };
      } else {
        // a dead seed is about the params, not the browser. press it again
        generateError = `no program passed the filter in ${res.attempts} tries`;
      }
    } catch (e) {
      generateError = String(e);
    } finally {
      generating = false;
    }
  };

  // same thing for the check box, only this one needs a program to look at first
  let generatingChecks = false;

  const generateChecks = async () => {
    generatingChecks = true;
    generateError = null;
    try {
      const { default: init, generate_checks } = await import('chip-wasm');
      await init();
      const seed = Math.floor(Math.random() * 2 ** 32);
      await new Promise((resolve) => setTimeout(resolve, 0));
      const res = generate_checks(program, seed, 5);
      if (res.error) {
        generateError = res.error;
      } else {
        checks = res.checks;
      }
    } catch (e) {
      generateError = String(e);
    } finally {
      generatingChecks = false;
    }
  };

  const prepareDot = (dot: string) =>
    dot
      .trim()
      .replaceAll('\n\n\n', '\n\n')
      .replaceAll(/"\]\[shape="doublecircle"/g, `",color="${mirage.syntax.tag.hex()}"`)
      .replaceAll(/\[label=[^\]]+\]\[shape="point"]/g, '[label="",opacity=0]')
      .replaceAll(/\]\[shape/g, ',shape');

  let pauseGraphRendering = false;

  // the program/check split, as a percent of the two editor boxes. the check bar is the handle
  const SPLIT_KEY = 'moka-split';
  const SPLIT_MIN = 15;
  const SPLIT_MAX = 85;
  let split = 66;
  let programPane: HTMLDivElement;
  let checkPane: HTMLDivElement;
  let dragging = false;
  // measured once on pointerdown so a drag is not doing layout reads every move
  let dragTop = 0;
  let dragSpan = 1;
  let grabOffset = 0;

  if (browser) {
    const saved = Number(localStorage.getItem(SPLIT_KEY));
    if (saved >= SPLIT_MIN && saved <= SPLIT_MAX) split = saved;
  }

  const clampSplit = (pct: number) => Math.min(SPLIT_MAX, Math.max(SPLIT_MIN, pct));

  const rememberSplit = () => {
    if (browser) localStorage.setItem(SPLIT_KEY, String(Math.round(split)));
  };

  const startResize = (e: PointerEvent) => {
    // the bar also holds the generate button, do not start a drag on it
    if ((e.target as HTMLElement).closest('button')) return;
    const top = programPane.getBoundingClientRect();
    const bottom = checkPane.getBoundingClientRect();
    dragTop = top.top;
    // the bars are a fixed height so the two boxes together stay the same all through the drag
    dragSpan = Math.max(top.height + bottom.height, 1);
    grabOffset = e.clientY - (e.currentTarget as HTMLElement).getBoundingClientRect().top;
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };

  const moveResize = (e: PointerEvent) => {
    if (!dragging) return;
    // subtract where in the bar it was grabbed so the bar stays under the cursor
    split = clampSplit(((e.clientY - grabOffset - dragTop) / dragSpan) * 100);
  };

  const endResize = (e: PointerEvent) => {
    if (!dragging) return;
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    rememberSplit();
  };

  // arrow keys too, a separator you can only drag is no good with a keyboard
  const keyResize = (e: KeyboardEvent) => {
    const step = e.key === 'ArrowUp' ? -4 : e.key === 'ArrowDown' ? 4 : 0;
    if (!step) return;
    e.preventDefault();
    split = clampSplit(split + step);
    rememberSplit();
  };
</script>

<svelte:head>
  <title>Moka</title>
  <meta name="description" content="Moka" />
</svelte:head>

<Nav title="Moka" {Icon} />

<div class="relative grid grid-cols-2 grid-rows-[2fr_auto] bg-slate-800">
  <div class="flex min-h-0 flex-col">
    <div class="flex items-center space-x-3 bg-slate-900 px-3 py-1.5 text-sm text-white">
      <button
        on:click={generateProgram}
        disabled={generating}
        class="font-bold transition hover:text-slate-300 disabled:opacity-50"
      >
        {generating ? 'Generating...' : 'Generate'}
      </button>
      <select
        bind:value={preset}
        disabled={generating}
        class="rounded-sm bg-slate-800 px-1 py-0.5 text-xs"
        title="which parameters to generate from"
      >
        <option value="default">default</option>
        <option value="stress">stress</option>
      </select>
    </div>
    <div bind:this={programPane} class="relative min-h-0" style="flex: {split} 1 0%">
      <Editor bind:value={program} markers={programMarkers} />
    </div>
    <!-- a focusable separator with a value is the aria splitter pattern, svelte just does not know it -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label="resize the check box"
      aria-valuenow={Math.round(100 - split)}
      aria-valuemin={100 - SPLIT_MAX}
      aria-valuemax={100 - SPLIT_MIN}
      tabindex="0"
      on:pointerdown={startResize}
      on:pointermove={moveResize}
      on:pointerup={endResize}
      on:pointercancel={endResize}
      on:keydown={keyResize}
      class="flex cursor-row-resize items-center space-x-3 bg-slate-900 px-3 py-1.5 text-sm text-white select-none {dragging
        ? 'bg-slate-700'
        : 'hover:bg-slate-800'}"
    >
      <span class="text-slate-400">Check</span>
      <button
        on:click={generateChecks}
        disabled={generatingChecks}
        class="font-bold transition hover:text-slate-300 disabled:opacity-50"
      >
        {generatingChecks ? 'Generating...' : 'Generate'}
      </button>
    </div>
    <div bind:this={checkPane} class="relative min-h-0" style="flex: {100 - split} 1 0%">
      <Editor bind:value={checks} bind:hoveredMarker markers={checkMarkers.map((c) => c.m)} />
    </div>
  </div>
  <div class="flex flex-col text-white">
    {#if false}
      {#each graphs as { title, dot }}
        {#if dot}
          <div class="flex flex-1 flex-col p-2">
            <h2 class="text-xl font-bold">{title}</h2>
            <div class="grid flex-1 grid-cols-1 overflow-auto rounded-sm bg-slate-700">
              {#if dot.includes('digraph')}
                <div class="m-1 rounded-sm bg-slate-900">
                  <Network dot={prepareDot(dot)} highlight={['n20']} />
                </div>
              {:else}
                <div class="relative">
                  <pre class="absolute inset-0 overflow-auto p-4">{prepareDot(dot)}</pre>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      {/each}
    {:else}
      <div class="relative flex-1">
        {#if pauseGraphRendering}
          <button
            class="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-sm border px-3 py-2 text-lg font-bold transition hover:bg-slate-500/10"
            on:click={() => (pauseGraphRendering = false)}
          >
            Graph rendering pause. Click to enable
          </button>
        {:else}
          <Network bind:hoveredNode dot={$result.ts_dot} highlight={highlightedNodes} />
        {/if}
        <div
          class="absolute right-1 top-1 flex items-center space-x-1 text-sm opacity-20 transition hover:opacity-100"
        >
          <label for="pause-graph-rendering" class="cursor-pointer select-none"
            >Pause graph rendering</label
          >
          <input
            type="checkbox"
            name="pause-graph-rendering"
            id="pause-graph-rendering"
            bind:checked={pauseGraphRendering}
          />
        </div>
      </div>
    {/if}
  </div>
  <div
    class="col-span-2 flex items-center p-2 text-2xl text-white transition duration-500 {$parseError
      ? 'bg-purple-600'
      : {
          idle: 'bg-gray-500',
          checking: 'bg-yellow-500',
          checked: 'bg-green-500',
          error: 'bg-red-500',
        }[$status]}"
  >
    <span class="font-bold">
      {$parseError
        ? 'Parse error'
        : {
            idle: 'Idle',
            checking: 'Checking...',
            checked: 'Checked',
            error: 'Error',
          }[$status]}
    </span>
    <div class="flex-1"></div>
    <span class="text-base">
      {#if generateError}
        {generateError}
      {:else if generated}
        seed <b>{generated.seed}</b> &middot;
        {generated.attempts}
        {generated.attempts == 1 ? 'candidate' : 'candidates'} drawn
      {/if}
      <!-- {#if !$parseError && $state == 'checked'}
          {#if $result.is_fully_annotated}
            The program is <b>fully annotated</b>
          {:else}
            The program is <b><i>not</i> fully annotated</b>
          {/if}
        {/if} -->
    </span>
  </div>
</div>
