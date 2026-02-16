import React from 'react';
import PropTypes from 'prop-types';
import { RetinaImg, Flexbox } from 'unifymail-component-kit';
import { localized } from 'unifymail-exports';
import { ConfigLike } from '../types';
import SystemTrayIconStore from '../../../system-tray/lib/system-tray-icon-store';
import AppEnv from '../../../app-env';

// -- Reusable UI Components for Settings --

const SectionHeading = ({ children }) => (
  <h2 className="text-xl font-bold text-white mb-4 mt-8">{children}</h2>
);

const SettingRow = ({ label, description, children, className = '' }) => (
  <div className={`flex items-center justify-between py-4 border-b border-gray-800 ${className}`}>
    <div className="flex-1 pr-4">
      <div className="text-base font-medium text-gray-200">{label}</div>
      {description && <div className="text-sm text-gray-500 mt-1">{description}</div>}
    </div>
    <div className="flex-shrink-0">{children}</div>
  </div>
);

const ToggleSwitch = ({ checked, onChange }) => (
  <button
    type="button"
    className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-purple-500 focus:ring-offset-2 focus:ring-offset-gray-900 ${checked ? 'bg-purple-600' : 'bg-gray-700'
      }`}
    onClick={() => onChange(!checked)}
  >
    <span
      className={`inline-block h-4 w-4 transform rounded-full bg-white transition duration-200 ease-in-out ${checked ? 'translate-x-6' : 'translate-x-1'
        }`}
    />
  </button>
);

const Select = ({ value, onChange, options }) => (
  <select
    value={value}
    onChange={e => onChange(e.target.value)}
    className="bg-gray-800 text-white border border-gray-700 rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-purple-500 focus:border-transparent"
  >
    {options.map(opt => (
      <option key={opt.value} value={opt.value}>
        {opt.label}
      </option>
    ))}
  </select>
);

// -- Converted Sub-Components --

class AppearanceScaleSlider extends React.Component<
  { id: string; config: ConfigLike },
  { value: number }
> {
  static displayName = 'AppearanceScaleSlider';

  kp = `core.workspace.interfaceZoom`;

  constructor(props) {
    super(props);
    this.state = { value: parseFloat(props.config.get(this.kp)) || 1 };
  }

  componentDidUpdate(prevProps) {
    if (prevProps.config !== this.props.config) {
      this.setState({ value: parseFloat(this.props.config.get(this.kp)) || 1 });
    }
  }

  handleChange = (e) => {
    const val = parseFloat(e.target.value);
    this.setState({ value: val });
    this.props.config.set(this.kp, val);
  };

  render() {
    return (
      <div className="flex items-center space-x-4 w-64">
        <span className="text-xs text-gray-500">A</span>
        <input
          type="range"
          min={0.8}
          max={1.4}
          step={0.05}
          value={this.state.value}
          onChange={this.handleChange}
          className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer accent-purple-600"
        />
        <span className="text-lg font-bold text-gray-300">A</span>
      </div>
    );
  }
}

class AppearanceModeSwitch extends React.Component<
  { id: string; config: ConfigLike },
  { value: string }
> {
  constructor(props) {
    super(props);
    this.state = {
      value: props.config.get('core.workspace.mode'),
    };
  }

  componentDidUpdate(prevProps) {
    if (prevProps.config !== this.props.config) {
      this.setState({ value: this.props.config.get('core.workspace.mode') });
    }
  }

  _onApplyChanges = () => {
    AppEnv.commands.dispatch(`navigation:${this.state.value}-mode-off`);
  };

  render() {
    const modes = [
      { id: 'list', label: localized('Single Panel') },
      { id: 'split', label: localized('Two Panel') },
      { id: 'splitVertical', label: localized('Two Panel Vertical') },
    ];

    const currentMode = this.state.value;
    const hasChanges = currentMode !== this.props.config.get('core.workspace.mode');

    return (
      <div className="flex flex-col space-y-3">
        <div className="flex space-x-2">
          {modes.map(mode => (
            <button
              key={mode.id}
              onClick={() => this.setState({ value: mode.id })}
              className={`px-4 py-2 rounded-md text-sm font-medium transition-colors ${currentMode === mode.id
                  ? 'bg-purple-600 text-white'
                  : 'bg-gray-800 text-gray-300 hover:bg-gray-700'
                }`}
            >
              {mode.label}
            </button>
          ))}
        </div>
        {hasChanges && (
          <button
            onClick={this._onApplyChanges}
            className="self-start px-4 py-1.5 bg-green-600 hover:bg-green-700 text-white text-xs font-bold rounded shadow-sm"
          >
            {localized('Apply Layout Changes')}
          </button>
        )}
      </div>
    );
  }
}

// -- Main Component --

class PreferencesAppearance extends React.Component<{ config: ConfigLike; configSchema: any }> {
  static displayName = 'PreferencesAppearance';

  onPickTheme = () => {
    AppEnv.commands.dispatch('window:launch-theme-picker');
  };

  render() {
    const { config } = this.props;

    return (
      <div className="min-h-full bg-gray-900 text-slate-200 p-8">

        <h1 className="text-3xl font-bold text-white mb-8">Appearance</h1>

        {/* Theme Section */}
        <SectionHeading>Theme</SectionHeading>
        <p className="text-gray-400 mb-6">Customize the look and feel of UnifyMail.</p>

        <div className="bg-gray-800/50 rounded-lg p-4 border border-gray-800 mb-6">
          <div className="flex items-center justify-between">
            <div>
              <div className="font-medium text-white mb-2">Current Theme</div>
              <div className="text-sm text-gray-400">Manage installed themes</div>
            </div>
            <button
              onClick={this.onPickTheme}
              className="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded-md text-sm font-medium transition-colors"
            >
              {localized('Change Theme...')}
            </button>
          </div>
        </div>

        <SettingRow
          label="Sync with system"
          description="Automatically switch theme based on your system settings."
        >
          <ToggleSwitch checked={false} onChange={() => { }} />
          {/* Placeholder for now */}
        </SettingRow>

        {/* Accessibility Section */}
        <SectionHeading>Accessibility</SectionHeading>
        <p className="text-gray-400 mb-6">Improve your experience by adapting the application to your needs.</p>

        <SettingRow label="Font family" description="Choose the font used throughout the interface.">
          <Select
            value="system"
            onChange={() => { }}
            options={[
              { value: 'system', label: 'System Default' },
              { value: 'inter', label: 'Inter' },
              { value: 'roboto', label: 'Roboto' }
            ]}
          />
        </SettingRow>

        <SettingRow label="Font size (Zoom)" description="Adjust the scaling of the entire interface.">
          <AppearanceScaleSlider id="scale-slider" config={config} />
        </SettingRow>

        <SettingRow label="Default scrollbars" description="Use standard OS scrollbars instead of custom slim ones.">
          <ToggleSwitch checked={false} onChange={() => { }} />
        </SettingRow>

        <SettingRow label="Disable animations" description="Reduce motion for a more static experience.">
          <ToggleSwitch checked={false} onChange={() => { }} />
        </SettingRow>


        {/* Layout & Behavior (Legacy Settings styled) */}
        <SectionHeading>Layout & Behavior</SectionHeading>

        <SettingRow label="Window Layout" description="Choose between single or multi-panel layouts.">
          <AppearanceModeSwitch id="layout-switch" config={config} />
        </SettingRow>

        <SettingRow label="Tray Icon Style" description="Customize how the tray icon indicates unread messages.">
          <div className="flex flex-col space-y-2">
            {['blue', 'red'].map(style => (
              <label key={style} className="inline-flex items-center">
                <input
                  type="radio"
                  className="form-radio text-purple-600 bg-gray-700 border-gray-600 focus:ring-purple-500"
                  name="trayIconStyle"
                  value={style}
                  checked={config.get('core.workspace.trayIconStyle') === style}
                  onChange={(e) => config.set('core.workspace.trayIconStyle', e.target.value)}
                />
                <span className="ml-2 text-sm text-gray-300">
                  {style === 'blue' ? 'Blue (Unread)' : 'Red (New) / Blue (Unread)'}
                </span>
              </label>
            ))}
          </div>
        </SettingRow>

        {process.platform === 'linux' && (
          <SettingRow label="Menu Bar Style" description="Linux specific window controls.">
            <div className="text-sm text-gray-500">Configuration available for Linux users. Use default for best experience.</div>
          </SettingRow>
        )}

      </div>
    );
  }
}

export default PreferencesAppearance;
